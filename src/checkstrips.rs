use crate::models::Title;
use crate::schema::episodes::dsl as e;
use crate::schema::titles::dsl as t;
use chrono::NaiveDate;
use diesel::dsl::sql;
use diesel::pg::PgConnection;
use diesel::prelude::*;
use failure::{format_err, Error};

pub fn check_strips(db: &PgConnection) -> Result<(), Error> {
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
            return Err(format_err!(
                "Expected daystrips for {} ({}) not found",
                title.title,
                title.slug,
            ));
        }
        if !title.has_daystrip() && daystrip.is_some() {
            return Err(format_err!(
                "Unexpected daystrips for {} ({}) found",
                title.title,
                title.slug,
            ));
        }

        if title.has_sundays() && sundays.is_none() {
            return Err(format_err!(
                "Expected sundays for {} ({}) not found",
                title.title,
                title.slug,
            ));
        }
        if !title.has_sundays() && sundays.is_some() {
            return Err(format_err!(
                "Unexpected sundays for {} ({}) found",
                title.title,
                title.slug,
            ));
        }
    }
    eprintln!("Daystrips and sundays checks out.");
    Ok(())
}
