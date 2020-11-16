use crate::models::Title;
use crate::schema::episodes::dsl as e;
use crate::schema::titles::dsl as t;
use anyhow::Result;
use chrono::NaiveDate;
use diesel::dsl::sql;
use diesel::pg::PgConnection;
use diesel::prelude::*;
use std::fmt::{self, Display};

pub fn check_strips(db: &PgConnection) -> Result<()> {
    let data = t::titles
        .left_join(e::episodes)
        .select((
            t::titles::all_columns(),
            sql(
                "max(case when orig_sundays then null else orig_to_date end)",
            ),
            sql(
                "max(case when orig_sundays then orig_to_date else null end)",
            ),
        ))
        .group_by(t::titles::all_columns())
        .load::<(Title, Option<NaiveDate>, Option<NaiveDate>)>(db)?;
    for (title, daystrip, sundays) in data {
        if title.has_daystrip() && daystrip.is_none() {
            return Err(Error::MissingDaystrips(title).into());
        }
        if !title.has_daystrip() && daystrip.is_some() {
            return Err(Error::UnexpectedDaystrips(title).into());
        }

        if title.has_sundays() && sundays.is_none() {
            return Err(Error::MissingSundays(title).into());
        }
        if !title.has_sundays() && sundays.is_some() {
            return Err(Error::UnexpectedSundays(title).into());
        }
    }
    eprintln!("Daystrips and sundays checks out.");
    Ok(())
}

#[derive(Debug)]
enum Error {
    MissingDaystrips(Title),
    UnexpectedDaystrips(Title),
    MissingSundays(Title),
    UnexpectedSundays(Title),
}
impl std::error::Error for Error {}
impl Display for Error {
    fn fmt(&self, out: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::MissingDaystrips(title) => write!(
                out,
                "Expected daystrips for {} ({}) not found",
                title.title, title.slug,
            ),
            Error::UnexpectedDaystrips(title) => write!(
                out,
                "Unexpected daystrips for {} ({}) found",
                title.title, title.slug,
            ),
            Error::MissingSundays(title) => write!(
                out,
                "Expected sundays for {} ({}) not found",
                title.title, title.slug,
            ),
            Error::UnexpectedSundays(title) => write!(
                out,
                "Unexpected sundays for {} ({}) found",
                title.title, title.slug,
            ),
        }
    }
}
