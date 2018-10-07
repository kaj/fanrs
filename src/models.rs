#![allow(proc_macro_derive_resolution_fallback)]

use diesel;
use diesel::pg::PgConnection;
use diesel::prelude::*;
use diesel::result::Error;
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
    pub fn get_or_create(
        name: &str,
        db: &PgConnection,
    ) -> Result<Title, Error> {
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

#[derive(Debug, Queryable)]
pub struct Episode {
    id: i32,
    title_id: i32,
    episode: Option<String>,
    teaser: Option<String>,
    note: Option<String>,
    copyright: Option<String>,
}

impl Episode {
    pub fn get(
        title: &Title,
        name: Option<&str>,
        db: &PgConnection,
    ) -> Result<Option<Episode>, Error> {
        use schema::episodes::dsl;
        dsl::episodes
            .filter(dsl::title.eq(title.id))
            .filter(dsl::episode.eq(name))
            .first::<Episode>(db)
            .optional()
    }
    pub fn create(
        title: &Title,
        name: Option<&str>,
        teaser: Option<&str>,
        note: Option<&str>,
        copyright: Option<&str>,
        db: &PgConnection,
    ) -> Result<Episode, Error> {
        use schema::episodes::dsl;
        diesel::insert_into(dsl::episodes)
            .values((
                dsl::title.eq(title.id),
                dsl::episode.eq(name),
                dsl::teaser.eq(teaser),
                dsl::note.eq(note),
                dsl::copyright.eq(copyright),
            ))
            .get_result(db)
    }
    pub fn set_details(
        self,
        teaser: Option<&str>,
        note: Option<&str>,
        copyright: Option<&str>,
        db: &PgConnection,
    ) -> Result<Episode, Error> {
        use schema::episodes::dsl;
        diesel::update(dsl::episodes.filter(dsl::id.eq(self.id)))
            .set((
                dsl::teaser.eq(teaser),
                dsl::note.eq(note),
                dsl::copyright.eq(copyright),
            ))
            .get_result(db)
    }
}
