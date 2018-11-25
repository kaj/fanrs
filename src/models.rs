#![allow(proc_macro_derive_resolution_fallback)]

use bigdecimal::BigDecimal;
use diesel;
use diesel::pg::PgConnection;
use diesel::prelude::*;
use diesel::result::Error;
use slug::slugify;
use std::fmt;
use xmltree::Element;

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

impl Issue {
    pub fn get_or_create(
        year: i16,
        number: i16,
        number_str: &str,
        pages: Option<i16>,
        price: Option<BigDecimal>,
        db: &PgConnection,
    ) -> Result<Issue, Error> {
        use schema::issues::dsl;
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
        use schema::publications::dsl as p;
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

/// A title of a comic.
///
/// May be recurring, such as "Fantomen" or "Spirit", or a one-shot.
#[derive(Debug, Queryable)]
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
    pub episode: Option<String>,
    pub teaser: Option<String>,
    pub note: Option<String>,
    pub copyright: Option<String>,
}

impl Episode {
    pub fn get_or_create(
        title: &Title,
        name: Option<&str>,
        teaser: Option<&str>,
        note: Option<&str>,
        copyright: Option<&str>,
        db: &PgConnection,
    ) -> Result<Episode, Error> {
        use schema::episodes::dsl;
        dsl::episodes
            .filter(dsl::title.eq(title.id))
            .filter(dsl::episode.eq(name))
            .first::<Episode>(db)
            .optional()?
            .map(|episode| episode.set_details(teaser, note, copyright, db))
            .unwrap_or_else(|| {
                diesel::insert_into(dsl::episodes)
                    .values((
                        dsl::title.eq(title.id),
                        dsl::episode.eq(name),
                        dsl::teaser.eq(teaser),
                        dsl::note.eq(note),
                        dsl::copyright.eq(copyright),
                    ))
                    .get_result(db)
            })
    }
    fn set_details(
        self,
        teaser: Option<&str>,
        note: Option<&str>,
        copyright: Option<&str>,
        db: &PgConnection,
    ) -> Result<Episode, Error> {
        use schema::episodes::dsl;
        let q = diesel::update(dsl::episodes.filter(dsl::id.eq(self.id)));
        match (teaser, note, copyright) {
            (Some(teaser), Some(note), Some(copyright)) => q
                .set((
                    dsl::teaser.eq(teaser),
                    dsl::note.eq(note),
                    dsl::copyright.eq(copyright),
                ))
                .get_result(db),
            (Some(teaser), Some(note), None) => q
                .set((dsl::teaser.eq(teaser), dsl::note.eq(note)))
                .get_result(db),
            (Some(teaser), None, Some(copyright)) => q
                .set((dsl::teaser.eq(teaser), dsl::copyright.eq(copyright)))
                .get_result(db),
            (Some(teaser), None, None) => {
                q.set(dsl::teaser.eq(teaser)).get_result(db)
            }
            (None, Some(note), Some(copyright)) => q
                .set((dsl::note.eq(note), dsl::copyright.eq(copyright)))
                .get_result(db),
            (None, Some(note), None) => {
                q.set(dsl::note.eq(note)).get_result(db)
            }
            (None, None, Some(copyright)) => {
                q.set(dsl::copyright.eq(copyright)).get_result(db)
            }
            (None, None, None) => Ok(self),
        }
    }
    /// A specific part of this episode (None for the whole episode) is
    /// published in a specific issue.
    /// TODO Handle best_plac
    pub fn publish_part(
        &self,
        part: Option<&Part>,
        issue: i32,
        seqno: Option<i16>,
        db: &PgConnection,
    ) -> Result<(), Error> {
        use schema::episode_parts::dsl as e;
        let part_no = part.and_then(|p| p.no.map(i16::from));
        let part_name = part.and_then(|p| p.name.as_ref());
        let epq = e::episode_parts
            .select(e::id)
            .filter(e::episode.eq(self.id))
            .into_boxed();
        let epq = if let Some(part_no) = part_no {
            epq.filter(e::part_no.eq(part_no))
        } else {
            epq.filter(e::part_no.is_null())
        };
        let epq = if let Some(part_name) = part_name {
            epq.filter(e::part_name.eq(part_name))
        } else {
            epq.filter(e::part_name.is_null())
        };

        let part_id = if let Some(part_id) =
            epq.first::<i32>(db).optional()?
        {
            part_id
        } else {
            diesel::insert_into(e::episode_parts)
                .values((
                    e::episode.eq(self.id),
                    e::part_no.eq(part_no),
                    e::part_name.eq(part_name),
                ))
                .get_result::<(i32, i32, Option<i16>, Option<String>, Option<i16>)>(db)?.0
        };
        use schema::publications::dsl as p;
        if let Some((id, old_seqno)) = p::publications
            .filter(p::issue.eq(issue))
            .filter(p::episode_part.eq(part_id))
            .select((p::id, p::seqno))
            .first::<(i32, Option<i16>)>(db)
            .optional()?
        {
            if seqno.is_some() && old_seqno != seqno {
                eprintln!("TODO: Should update seqno for {}", id);
            }
            Ok(())
        } else {
            diesel::insert_into(p::publications)
                .values((
                    p::issue.eq(issue),
                    p::episode_part.eq(part_id),
                    p::seqno.eq(seqno),
                ))
                .execute(db)?;
            Ok(())
        }
    }
}

#[derive(Debug, Queryable)]
pub struct Part {
    no: Option<i16>,
    name: Option<String>,
}

impl Part {
    pub fn of(elem: &Element) -> Option<Self> {
        elem.get_child("part").map(|e| Part {
            no: e.attributes.get("no").and_then(|n| n.parse().ok()),
            name: e.text.clone(),
        })
    }
}

impl fmt::Display for Part {
    fn fmt(&self, out: &mut fmt::Formatter) -> fmt::Result {
        if let Some(no) = self.no {
            write!(out, "del {}", no)?;
            if self.name.is_some() {
                write!(out, ":")?;
            }
        }
        if let Some(ref name) = self.name {
            write!(out, "{}", name)?;
        }
        Ok(())
    }
}
