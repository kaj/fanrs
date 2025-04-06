use super::price::Price;
use crate::templates::ToHtml;
use diesel::deserialize::{self, FromSql};
use diesel::pg::Pg;
use diesel::prelude::*;
use diesel::result::Error;
use diesel::sql_types::{SmallInt, Text};
use diesel_async::{AsyncPgConnection, RunQueryDsl};
use std::cmp::Ordering;
use std::fmt;
use std::io::{self, Write};
use std::str::FromStr;

#[derive(Debug, Queryable)]
pub struct Issue {
    pub id: i32,
    pub year: i16,
    pub number: i16,
    pub number_str: String,
    pub pages: Option<i16>,
    pub price: Option<Price>,
    pub cover_best: Option<i16>,
    pub magic: i16,
    pub ord: Option<i32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IssueRef {
    pub year: i16,
    pub number: Nr,
}

impl Issue {
    pub async fn get_or_create_ref(
        year: i16,
        number: Nr,
        db: &mut AsyncPgConnection,
    ) -> Result<Issue, Error> {
        match Issue::load(year, &number, db).await? {
            Some(t) => Ok(t),
            None => {
                Issue::create(year, number, None, None, None, None, db).await
            }
        }
    }
    pub async fn get_or_create(
        year: i16,
        number: Nr,
        ord: Option<i32>,
        pages: Option<i16>,
        price: Option<Price>,
        cover_best: Option<i16>,
        db: &mut AsyncPgConnection,
    ) -> Result<Issue, Error> {
        use crate::schema::issues::dsl;
        if let Some(mut t) = Issue::load(year, &number, db).await? {
            if (t.cover_best != cover_best)
                || (t.pages != pages)
                || (t.price != price)
                || (t.ord != ord)
            {
                t.cover_best = cover_best;
                t.pages = pages;
                t.price = price;
                t.ord = ord;
                diesel::update(dsl::issues)
                    .filter(dsl::id.eq(t.id))
                    .set((
                        dsl::cover_best.eq(cover_best),
                        dsl::pages.eq(pages),
                        dsl::price.eq(&t.price),
                        dsl::ord.eq(t.ord),
                    ))
                    .execute(db)
                    .await?;
            }
            Ok(t)
        } else {
            Issue::create(year, number, ord, pages, price, cover_best, db)
                .await
        }
    }
    async fn load(
        year: i16,
        number: &Nr,
        db: &mut AsyncPgConnection,
    ) -> Result<Option<Issue>, Error> {
        use crate::schema::issues::dsl as i;
        i::issues
            .filter(i::year.eq(year))
            .filter(i::number.eq(number.number))
            .filter(i::number_str.eq(&number.nr_str))
            .first::<Issue>(db)
            .await
            .optional()
    }
    async fn create(
        year: i16,
        number: Nr,
        ord: Option<i32>,
        pages: Option<i16>,
        price: Option<Price>,
        cover_best: Option<i16>,
        db: &mut AsyncPgConnection,
    ) -> Result<Issue, Error> {
        use crate::schema::issues::dsl as i;
        let magic = ((year - 1950) * 64 + number.number) * 2
            + i16::from(number.nr_str.contains('-'));
        diesel::insert_into(i::issues)
            .values((
                i::year.eq(year),
                i::number.eq(number.number),
                i::number_str.eq(number.nr_str),
                i::pages.eq(pages),
                i::price.eq(price),
                i::cover_best.eq(cover_best),
                i::magic.eq(magic),
                i::ord.eq(ord),
            ))
            .get_result(db)
            .await
    }
    pub async fn clear(
        &self,
        db: &mut AsyncPgConnection,
    ) -> Result<(), Error> {
        use crate::schema::publications::dsl as p;
        diesel::delete(p::publications.filter(p::issue_id.eq(self.id)))
            .execute(db)
            .await?;
        Ok(())
    }
    /// Site-relative url to the cover image of this issue.
    pub fn cover_url(&self) -> String {
        format!("/c/f{}-{}.jpg", self.year, self.number)
    }
}

impl fmt::Display for Issue {
    fn fmt(&self, out: &mut fmt::Formatter) -> fmt::Result {
        write!(out, "{}/{}", self.number_str, self.year)?;
        match (&self.pages, &self.price) {
            (Some(pages), Some(price)) => {
                write!(out, " ({pages} sidor, pris {price})")
            }
            (Some(pages), None) => write!(out, " ({pages} sidor)"),
            (None, Some(price)) => write!(out, " (pris {price})"),
            (None, None) => Ok(()),
        }
    }
}

#[derive(Debug)]
pub enum ParseError {
    BadIssue,
    BadYear,
    NoSpace,
}
impl std::fmt::Display for ParseError {
    fn fmt(&self, out: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            ParseError::BadIssue => write!(out, "Bad issue in input"),
            ParseError::BadYear => write!(out, "Bad year in input"),
            ParseError::NoSpace => write!(out, "Space missing in input"),
        }
    }
}
impl std::error::Error for ParseError {}

impl IssueRef {
    pub fn from_magic(n: i16) -> IssueRef {
        let double = (n % 2) > 0;
        let n = n / 2;
        let number = n % 64;
        let year = 1950 + n / 64;
        IssueRef {
            year,
            number: Nr {
                number,
                nr_str: if double {
                    format!("{number}-{}", number + 1)
                } else {
                    format!("{number}")
                },
            },
        }
    }

    pub async fn load_id(
        &self,
        db: &mut AsyncPgConnection,
    ) -> Result<i32, Error> {
        use crate::schema::issues::dsl as i;
        i::issues
            .select(i::id)
            .filter(i::year.eq(self.year))
            .filter(i::number.eq(self.number.number))
            .filter(i::number_str.eq(&self.number.nr_str))
            .first(db)
            .await
    }

    pub fn sortno(&self) -> i16 {
        (self.year - 1950) * 64 + self.number.number
    }

    /// Site-relative url to the cover image of this issue.
    pub fn cover_url(&self) -> String {
        format!("/c/f{}-{}.jpg", self.year, self.number.number)
    }
}

impl FromSql<SmallInt, Pg> for IssueRef {
    fn from_sql(
        bytes: <Pg as diesel::backend::Backend>::RawValue<'_>,
    ) -> diesel::deserialize::Result<Self> {
        FromSql::<SmallInt, Pg>::from_sql(bytes).map(IssueRef::from_magic)
    }
}

impl Queryable<SmallInt, Pg> for IssueRef {
    type Row = i16;
    fn build(row: Self::Row) -> deserialize::Result<IssueRef> {
        Ok(IssueRef::from_magic(row))
    }
}

impl Queryable<(SmallInt, (SmallInt, Text)), Pg> for IssueRef {
    type Row = (i16, (i16, String));
    fn build(row: Self::Row) -> deserialize::Result<IssueRef> {
        Ok(IssueRef {
            year: row.0,
            number: <Nr as Queryable<(SmallInt, Text), Pg>>::build(row.1)?,
        })
    }
}

impl FromStr for IssueRef {
    type Err = ParseError;
    fn from_str(s: &str) -> Result<IssueRef, Self::Err> {
        if let Some((Some(year), Some(nr))) =
            s.find(' ').map(|p| (s.get(0..p), s.get(p + 1..)))
        {
            Ok(IssueRef {
                year: year.parse().map_err(|_| ParseError::BadYear)?,
                number: nr.parse()?,
            })
        } else {
            Err(ParseError::NoSpace)
        }
    }
}

impl Ord for IssueRef {
    fn cmp(&self, rhs: &IssueRef) -> Ordering {
        self.year
            .cmp(&rhs.year)
            .then_with(|| self.number.cmp(&rhs.number))
    }
}
impl PartialOrd for IssueRef {
    fn partial_cmp(&self, rhs: &IssueRef) -> Option<Ordering> {
        Some(self.cmp(rhs))
    }
}

impl ToHtml for IssueRef {
    fn to_html(&self, out: &mut dyn Write) -> io::Result<()> {
        write!(
            out,
            "<a href='/{y}/{n}'><span class='ifwide'>Fa</span> \
             {ns}\u{200b}/{y}</a>",
            y = self.year,
            n = self.number.number,
            ns = self.number.nr_str,
        )
    }
}

/// A number of an issue (excluding year).
#[derive(Clone, Debug, PartialEq, Eq, Queryable)]
pub struct Nr {
    number: i16,
    nr_str: String,
}
impl Nr {
    pub fn first(&self) -> i16 {
        self.number
    }
    pub fn last(&self) -> i16 {
        if self.nr_str.contains('-') {
            self.number + 1
        } else {
            self.number
        }
    }
}

impl fmt::Display for Nr {
    fn fmt(&self, out: &mut fmt::Formatter) -> fmt::Result {
        self.nr_str.fmt(out)
    }
}

impl FromStr for Nr {
    type Err = ParseError;
    fn from_str(nr_str: &str) -> Result<Nr, Self::Err> {
        let number = nr_str
            .find('-')
            .map_or(nr_str, |p| &nr_str[0..p])
            .parse()
            .map_err(|_| ParseError::BadIssue)?;
        Ok(Nr {
            number,
            nr_str: nr_str.to_string(),
        })
    }
}

impl Ord for Nr {
    fn cmp(&self, rhs: &Nr) -> Ordering {
        self.number.cmp(&rhs.number)
    }
}
impl PartialOrd for Nr {
    fn partial_cmp(&self, rhs: &Nr) -> Option<Ordering> {
        Some(self.cmp(rhs))
    }
}
