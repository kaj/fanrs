use super::{Part, RefKey, Title};
use diesel;
use diesel::pg::PgConnection;
use diesel::prelude::*;
use diesel::result::Error;

#[derive(Debug, Queryable)]
pub struct Episode {
    pub id: i32,
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
        use crate::schema::episodes::dsl;
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
        use crate::schema::episodes::dsl;
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

    pub fn set_refs(
        &self,
        refs: &[RefKey],
        db: &PgConnection,
    ) -> Result<(), Error> {
        for r in refs {
            let id = r.get_or_create_id(db)?;
            use crate::schema::episode_refkeys::dsl as er;
            diesel::insert_into(er::episode_refkeys)
                .values((er::episode_id.eq(self.id), er::refkey_id.eq(id)))
                .on_conflict_do_nothing()
                .execute(db)?;
        }
        Ok(())
    }

    /// A specific part of this episode (None for the whole episode) is
    /// published in a specific issue.
    pub fn publish_part(
        &self,
        part: Option<&Part>,
        issue: i32,
        seqno: Option<i16>,
        best_plac: Option<i16>,
        db: &PgConnection,
    ) -> Result<(), Error> {
        use crate::schema::episode_parts::dsl as e;
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
                .get_result::<(i32, i32, Option<i16>, Option<String>)>(db)?
                .0
        };
        use crate::schema::publications::dsl as p;
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
                    p::best_plac.eq(best_plac),
                ))
                .execute(db)?;
            Ok(())
        }
    }
}
