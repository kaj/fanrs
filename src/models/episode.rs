use super::{Part, RefKey, Title};
use crate::templates::ToHtml;
use chrono::{Datelike, NaiveDate};
use diesel;
use diesel::pg::PgConnection;
use diesel::prelude::*;
use diesel::result::Error;
use std::fmt;
use std::io::{self, Write};

#[derive(Debug, Queryable)]
pub struct Episode {
    pub id: i32,
    title_id: i32,
    pub episode: Option<String>,
    pub teaser: Option<String>,
    pub note: Option<String>,
    pub copyright: Option<String>,
    orig_lang: Option<String>,
    orig_episode: Option<String>,
    pub orig_date: Option<NaiveDate>,
    pub orig_to_date: Option<NaiveDate>,
    pub sun: bool,
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
    /// Return original language and title, if known.
    pub fn orig(&self) -> Option<OrigEpisode> {
        if let (Some(lang), Some(episode)) =
            (&self.orig_lang, &self.orig_episode)
        {
            Some(OrigEpisode {
                lang: &lang,
                episode: &episode,
            })
        } else {
            None
        }
    }
    pub fn orig_dates(&self) -> OrigDates {
        OrigDates {
            from: self.orig_date,
            to: self.orig_to_date,
            sun: self.sun,
        }
    }
}

pub struct OrigEpisode<'a> {
    lang: &'a str,
    episode: &'a str,
}

impl<'a> ToHtml for OrigEpisode<'a> {
    fn to_html(&self, out: &mut Write) -> io::Result<()> {
        write!(
            out,
            "{} originalets titel: <i lang='{}'>",
            match self.lang {
                "fr" => "Franska",
                "en" => "Engelska",
                l => l,
            },
            self.lang,
        )?;
        self.episode.to_html(out)?;
        out.write_all(b"</i>")
    }
}

pub struct OrigDates {
    from: Option<NaiveDate>,
    to: Option<NaiveDate>,
    sun: bool,
}

impl OrigDates {
    pub fn date(date: NaiveDate) -> Self {
        OrigDates {
            from: Some(date),
            to: None,
            sun: false,
        }
    }
}

impl ToHtml for OrigDates {
    fn to_html(&self, out: &mut Write) -> io::Result<()> {
        match (self.from, self.to) {
            (Some(from), Some(to)) => write!(
                out,
                "<p class='info dates'>{} {} - {}.</p>",
                if self.sun {
                    "Söndagssidor"
                } else {
                    "Dagstrippar"
                },
                SvDate(&from),
                SvDate(&to),
            ),
            (Some(date), None) => write!(
                out,
                "<p class='info date'>Först publicerad {}.</p>",
                SvDate(&date),
            ),
            (None, _) => Ok(()),
        }
    }
}

struct SvDate<'a>(&'a NaiveDate);

impl<'a> fmt::Display for SvDate<'a> {
    fn fmt(&self, out: &mut fmt::Formatter) -> fmt::Result {
        write!(
            out,
            "{} den {} {} {}",
            LONG_WEEKDAYS[self.0.weekday().num_days_from_monday() as usize],
            self.0.day(),
            LONG_MONTHS[(self.0.month() - 1) as usize],
            self.0.year(),
        )
    }
}

static LONG_WEEKDAYS: [&'static str; 7] = [
    "måndag", "tisdag", "onsdag", "torsdag", "fredag", "lördag", "söndag",
];
static LONG_MONTHS: [&'static str; 12] = [
    "januari",
    "februari",
    "mars",
    "april",
    "maj",
    "juni",
    "juli",
    "augusti",
    "september",
    "oktober",
    "november",
    "december",
];
