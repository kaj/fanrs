use crate::models::Title;
use crate::schema::episodes::dsl as e;
use crate::schema::titles::dsl as t;
use anyhow::Result;
use diesel::dsl::sql;
use diesel::prelude::*;
use diesel::sql_types::Bool;
use diesel_async::{AsyncPgConnection, RunQueryDsl};
use std::fmt::{self, Display};

pub async fn check_strips(db: &mut AsyncPgConnection) -> Result<()> {
    let data = t::titles
        .left_join(e::episodes)
        .select((
            Title::as_select(),
            sql::<Bool>("bool_or(orig_to_date is not null and orig_sundays)"),
            sql::<Bool>(
                "bool_or(orig_to_date is not null and not orig_sundays)",
            ),
        ))
        .group_by(t::titles::all_columns())
        .load::<(Title, bool, bool)>(db)
        .await?;
    for (title, daystrip, sundays) in data {
        if title.has_daystrip() && !daystrip {
            return Err(Error::MissingDaystrips(title).into());
        }
        if !title.has_daystrip() && daystrip {
            return Err(Error::UnexpectedDaystrips(title).into());
        }

        if title.has_sundays() && !sundays {
            return Err(Error::MissingSundays(title).into());
        }
        if !title.has_sundays() && sundays {
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
