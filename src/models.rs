#![allow(proc_macro_derive_resolution_fallback)]

use diesel;
use diesel::pg::PgConnection;
use diesel::prelude::*;
use super::Error;
use slug::slugify;

/// A title of a comic.
///
/// May be recurring, such as "Fantomen" or "Spirit", or a one-shot.
#[derive(Debug, Queryable)]
pub struct Title {
    id: i32,
    title: String,
    slug: String,
}

impl Title {
    pub fn get_or_create(name: &str, db: &PgConnection) -> Result<Title, Error> {
        use schema::titles::dsl::*;
        if let Some(t) = titles
            .filter(title.eq(name))
            .first::<Title>(db)
            .optional()?
        {
            Ok(t)
        } else {
            Ok(diesel::insert_into(titles)
               .values((title.eq(name), slug.eq(&slugify(name))))
               .get_result(db)?)
        }
    }
}
