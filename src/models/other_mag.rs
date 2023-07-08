use crate::schema::other_mags::dsl as om;
use diesel::prelude::*;
use diesel::result::Error;
use diesel_async::{AsyncPgConnection, RunQueryDsl};

#[derive(Debug, Queryable, PartialOrd, Ord, PartialEq, Eq)]
pub struct OtherMag {
    pub id: i32,
    name: String,
    issue: Option<i16>,
    i_of: Option<i16>,
    year: Option<i16>,
}

impl OtherMag {
    pub async fn get_by_id(
        id: i32,
        db: &mut AsyncPgConnection,
    ) -> Result<OtherMag, Error> {
        om::other_mags.filter(om::id.eq(id)).first(db).await
    }

    pub async fn get_or_create(
        name: String,
        issue: Option<i16>,
        i_of: Option<i16>,
        year: Option<i16>,
        db: &mut AsyncPgConnection,
    ) -> Result<OtherMag, Error> {
        if let Some(m) = om::other_mags
            .filter(om::name.eq(&name))
            .filter(om::issue.is_not_distinct_from(&issue))
            .filter(om::i_of.is_not_distinct_from(&i_of))
            .filter(om::year.is_not_distinct_from(&year))
            .first::<OtherMag>(db)
            .await
            .optional()?
        {
            Ok(m)
        } else {
            diesel::insert_into(om::other_mags)
                .values((
                    om::name.eq(name),
                    om::issue.eq(issue),
                    om::i_of.eq(i_of),
                    om::year.eq(year),
                ))
                .get_result(db)
                .await
        }
    }
}

use std::fmt::{self, Display};

impl Display for OtherMag {
    fn fmt(&self, out: &mut fmt::Formatter) -> fmt::Result {
        self.name.fmt(out)?;
        if let Some(issue) = self.issue {
            write!(out, " nr {issue}")?;
            if let Some(i_of) = self.i_of {
                write!(out, "/{i_of}")?;
            }
        }
        if let Some(year) = self.year {
            write!(out, " {year}")?;
        }
        Ok(())
    }
}
