use crate::schema::titles::dsl as t;
use diesel;
use diesel::pg::PgConnection;
use diesel::prelude::*;
use diesel::result::Error;
use slug::slugify;
use std::cmp::Ordering;

/// A title of a comic.
///
/// May be recurring, such as "Fantomen" or "Spirit", or a one-shot.
#[derive(Debug, Queryable, Ord, PartialEq, Eq)]
pub struct Title {
    pub id: i32,
    pub title: String,
    pub slug: String,
}

impl Title {
    pub fn get_or_create(
        name: &str,
        db: &PgConnection,
    ) -> Result<Title, Error> {
        if let Some(t) = t::titles
            .filter(t::title.eq(name))
            .first::<Title>(db)
            .optional()?
        {
            Ok(t)
        } else {
            Ok(diesel::insert_into(t::titles)
                .values((t::title.eq(name), t::slug.eq(&slugify(name))))
                .get_result(db)?)
        }
    }

    pub fn from_slug(slug: &str, db: &PgConnection) -> Result<Title, Error> {
        t::titles.filter(t::slug.eq(slug)).first(db)
    }
}

impl PartialOrd for Title {
    fn partial_cmp(&self, rhs: &Title) -> Option<Ordering> {
        Some(self.title.cmp(&rhs.title))
    }
}
