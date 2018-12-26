use crate::templates::ToHtml;
use bigdecimal::BigDecimal;
use diesel;
use diesel::pg::PgConnection;
use diesel::prelude::*;
use failure::Error;
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
    pub price: Option<BigDecimal>,
    pub cover_best: Option<i16>,
}

#[derive(Debug, Queryable)]
pub struct IssueRef {
    pub year: i16,
    pub number: i16,
    pub number_str: String,
}

impl Issue {
    pub fn get_or_create(
        year: i16,
        number: i16,
        number_str: &str,
        pages: Option<i16>,
        price: Option<BigDecimal>,
        db: &PgConnection,
    ) -> Result<Issue, Error> {
        use crate::schema::issues::dsl;
        if let Some(t) = dsl::issues
            .filter(dsl::year.eq(year))
            .filter(dsl::number.eq(number))
            .filter(dsl::number_str.eq(number_str))
            .first::<Issue>(db)
            .optional()?
        {
            Ok(t)
        } else {
            Ok(diesel::insert_into(dsl::issues)
                .values((
                    dsl::year.eq(year),
                    dsl::number.eq(number),
                    dsl::number_str.eq(number_str),
                    dsl::pages.eq(pages),
                    dsl::price.eq(price),
                ))
                .get_result(db)?)
        }
    }
    pub fn clear(&self, db: &PgConnection) -> Result<(), Error> {
        use crate::schema::publications::dsl as p;
        diesel::delete(p::publications.filter(p::issue.eq(self.id)))
            .execute(db)?;
        Ok(())
    }
}

impl fmt::Display for Issue {
    fn fmt(&self, out: &mut fmt::Formatter) -> fmt::Result {
        write!(out, "{}/{}", self.number_str, self.year)?;
        match (&self.pages, &self.price) {
            (Some(ref pages), Some(ref price)) => {
                write!(out, " ({} sidor, {})", pages, price)
            }
            (Some(ref pages), None) => write!(out, " ({} sidor)", pages),
            (None, Some(ref price)) => write!(out, " ({})", price),
            (None, None) => Ok(()),
        }
    }
}

pub fn parse_nr(nr_str: &str) -> Result<(i16, &str), ParseError> {
    let nr = nr_str
        .find('-')
        .map(|p| &nr_str[0..p])
        .unwrap_or(nr_str)
        .parse()
        .map_err(|_| ParseError::BadIssue)?;
    Ok((nr, nr_str))
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

impl FromStr for IssueRef {
    type Err = ParseError;
    fn from_str(s: &str) -> Result<IssueRef, Self::Err> {
        if let Some((Some(year), Some(nr))) =
            s.find(' ').map(|p| (s.get(0..p), s.get(p + 1..)))
        {
            let (nr, nr_str) =
                parse_nr(nr).map_err(|_| ParseError::BadIssue)?;
            let year = year.parse().map_err(|_| ParseError::BadYear)?;
            Ok(IssueRef {
                year,
                number: nr,
                number_str: nr_str.to_string(),
            })
        } else {
            Err(ParseError::NoSpace)
        }
    }
}

impl ToHtml for IssueRef {
    fn to_html(&self, out: &mut Write) -> io::Result<()> {
        write!(
            out,
            "<a href='/{y}#i{n}'>Fa {ns}/{y}</a>",
            y = self.year,
            n = self.number,
            ns = self.number_str,
        )
    }
}
