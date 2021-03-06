use super::price::Price;
use crate::templates::ToHtml;
use diesel::pg::{Pg, PgConnection};
use diesel::prelude::*;
use diesel::result::Error;
use diesel::sql_types::{SmallInt, Text};
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
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IssueRef {
    pub year: i16,
    pub number: Nr,
}

impl Issue {
    pub fn get_or_create_ref(
        year: i16,
        number: Nr,
        db: &PgConnection,
    ) -> Result<Issue, Error> {
        match Issue::load(year, &number, db)? {
            Some(t) => Ok(t),
            None => Issue::create(year, number, None, None, None, db),
        }
    }
    pub fn get_or_create(
        year: i16,
        number: Nr,
        pages: Option<i16>,
        price: Option<Price>,
        cover_best: Option<i16>,
        db: &PgConnection,
    ) -> Result<Issue, Error> {
        use crate::schema::issues::dsl;
        if let Some(mut t) = Issue::load(year, &number, db)? {
            if (t.cover_best != cover_best)
                || (t.pages != pages)
                || (t.price != price)
            {
                t.cover_best = cover_best;
                t.pages = pages;
                t.price = price;
                diesel::update(dsl::issues)
                    .filter(dsl::id.eq(t.id))
                    .set((
                        dsl::cover_best.eq(cover_best),
                        dsl::pages.eq(pages),
                        dsl::price.eq(&t.price),
                    ))
                    .execute(db)?;
            }
            Ok(t)
        } else {
            Issue::create(year, number, pages, price, cover_best, db)
        }
    }
    fn load(
        year: i16,
        number: &Nr,
        db: &PgConnection,
    ) -> Result<Option<Issue>, Error> {
        use crate::schema::issues::dsl as i;
        i::issues
            .filter(i::year.eq(year))
            .filter(i::number.eq(number.number))
            .filter(i::number_str.eq(&number.nr_str))
            .first::<Issue>(db)
            .optional()
    }
    fn create(
        year: i16,
        number: Nr,
        pages: Option<i16>,
        price: Option<Price>,
        cover_best: Option<i16>,
        db: &PgConnection,
    ) -> Result<Issue, Error> {
        use crate::schema::issues::dsl as i;
        let magic = ((year - 1950) * 64 + number.number) * 2
            + if number.nr_str.contains('-') { 1 } else { 0 };
        diesel::insert_into(i::issues)
            .values((
                i::year.eq(year),
                i::number.eq(number.number),
                i::number_str.eq(number.nr_str),
                i::pages.eq(pages),
                i::price.eq(price),
                i::cover_best.eq(cover_best),
                i::magic.eq(magic),
            ))
            .get_result(db)
    }
    pub fn clear(&self, db: &PgConnection) -> Result<(), Error> {
        use crate::schema::publications::dsl as p;
        diesel::delete(p::publications.filter(p::issue.eq(self.id)))
            .execute(db)?;
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
            (Some(ref pages), Some(ref price)) => {
                write!(out, " ({} sidor, pris {})", pages, price)
            }
            (Some(ref pages), None) => write!(out, " ({} sidor)", pages),
            (None, Some(ref price)) => write!(out, " (pris {})", price),
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
                    format!("{}-{}", number, number + 1)
                } else {
                    format!("{}", number)
                },
            },
        }
    }

    pub fn sortno(&self) -> i16 {
        (self.year - 1950) * 64 + self.number.number
    }

    /// Site-relative url to the cover image of this issue.
    pub fn cover_url(&self) -> String {
        format!("/c/f{}-{}.jpg", self.year, self.number.number)
    }
}

impl Queryable<SmallInt, Pg> for IssueRef {
    type Row = i16;
    fn build(row: Self::Row) -> IssueRef {
        IssueRef::from_magic(row)
    }
}

impl Queryable<(SmallInt, (SmallInt, Text)), Pg> for IssueRef {
    type Row = (i16, (i16, String));
    fn build(row: Self::Row) -> IssueRef {
        IssueRef {
            year: row.0,
            number: Nr {
                number: (row.1).0,
                nr_str: (row.1).1,
            },
        }
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
            .map(|p| &nr_str[0..p])
            .unwrap_or(nr_str)
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
