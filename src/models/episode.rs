use super::{OtherMag, RefKey, Title};
use crate::schema::episodes;
use crate::templates::ToHtml;
use chrono::{Datelike, NaiveDate};
use diesel::prelude::*;
use diesel::result::Error;
use diesel_async::{AsyncPgConnection, RunQueryDsl};
use std::fmt;
use std::io::{self, Write};
use tracing::warn;

#[derive(Debug, Identifiable, Queryable, Selectable)]
pub struct Episode {
    pub id: i32,
    #[allow(unused)]
    title_id: i32,
    pub name: Option<String>,
    pub teaser: Option<String>,
    pub note: Option<String>,
    pub copyright: Option<String>,
    orig_lang: Option<String>,
    orig_episode: Option<String>,
    orig_date: Option<NaiveDate>,
    orig_to_date: Option<NaiveDate>,
    orig_sundays: bool,
    orig_mag_id: Option<i32>,
    strip_from: Option<i32>,
    strip_to: Option<i32>,
}

impl Episode {
    pub async fn get_or_create(
        title: &Title,
        name: Option<&str>,
        teaser: Option<&str>,
        note: Option<&str>,
        copyright: Option<&str>,
        db: &mut AsyncPgConnection,
    ) -> Result<Episode, Error> {
        use crate::schema::episodes::dsl;
        match dsl::episodes
            .select(Episode::as_select())
            .filter(dsl::title_id.eq(title.id))
            .filter(dsl::name.eq(name))
            .first::<Episode>(db)
            .await
            .optional()?
        {
            Some(episode) => {
                episode.set_details(teaser, note, copyright, db).await
            }
            None => {
                diesel::insert_into(dsl::episodes)
                    .values((
                        dsl::title_id.eq(title.id),
                        dsl::name.eq(name),
                        dsl::teaser.eq(teaser),
                        dsl::note.eq(note),
                        dsl::copyright.eq(copyright),
                    ))
                    .get_result(db)
                    .await
            }
        }
    }
    async fn set_details(
        self,
        teaser: Option<&str>,
        note: Option<&str>,
        copyright: Option<&str>,
        db: &mut AsyncPgConnection,
    ) -> Result<Episode, Error> {
        use crate::schema::episodes::dsl;
        let q = diesel::update(dsl::episodes.filter(dsl::id.eq(self.id)));
        match (teaser, note, copyright) {
            (Some(teaser), Some(note), Some(copyright)) => {
                q.set((
                    dsl::teaser.eq(teaser),
                    dsl::note.eq(note),
                    dsl::copyright.eq(copyright),
                ))
                .get_result(db)
                .await
            }
            (Some(teaser), Some(note), None) => {
                q.set((dsl::teaser.eq(teaser), dsl::note.eq(note)))
                    .get_result(db)
                    .await
            }
            (Some(teaser), None, Some(copyright)) => {
                q.set((dsl::teaser.eq(teaser), dsl::copyright.eq(copyright)))
                    .get_result(db)
                    .await
            }
            (Some(teaser), None, None) => {
                q.set(dsl::teaser.eq(teaser)).get_result(db).await
            }
            (None, Some(note), Some(copyright)) => {
                q.set((dsl::note.eq(note), dsl::copyright.eq(copyright)))
                    .get_result(db)
                    .await
            }
            (None, Some(note), None) => {
                q.set(dsl::note.eq(note)).get_result(db).await
            }
            (None, None, Some(copyright)) => {
                q.set(dsl::copyright.eq(copyright)).get_result(db).await
            }
            (None, None, None) => Ok(self),
        }
    }

    pub async fn set_refs(
        &self,
        refs: &[RefKey],
        db: &mut AsyncPgConnection,
    ) -> Result<(), Error> {
        use crate::schema::episode_refkeys::dsl as er;
        let mut values = Vec::with_capacity(refs.len());
        for r in refs {
            let id = r.get_or_create_id(db).await?;
            values.push((er::episode_id.eq(self.id), er::refkey_id.eq(id)));
        }
        diesel::insert_into(er::episode_refkeys)
            .values(values)
            .on_conflict_do_nothing()
            .execute(db)
            .await?;
        Ok(())
    }

    /// Return original language and title, if known.
    pub fn orig(&self) -> Option<OrigEpisode> {
        if let (Some(lang), Some(episode)) =
            (&self.orig_lang, &self.orig_episode)
        {
            Some(OrigEpisode { lang, episode })
        } else {
            None
        }
    }
    pub fn orig_dates(&self) -> Option<OrigDates> {
        self.orig_date.map(|date| OrigDates {
            from: date,
            to: self.orig_to_date,
            sun: self.orig_sundays,
        })
    }
    pub fn strip_nrs(&self) -> Option<(i32, i32)> {
        match (self.strip_from, self.strip_to) {
            (Some(from), Some(to)) => Some((from, to)),
            (None, None) => None,
            (from, to) => {
                warn!(
                    "One-ended strips {from:?} - {to:?} in ep #{}",
                    self.id,
                );
                None
            }
        }
    }
    pub async fn load_orig_mag(
        &self,
        db: &mut AsyncPgConnection,
    ) -> Result<Option<OtherMag>, Error> {
        if let Some(id) = self.orig_mag_id {
            Ok(Some(OtherMag::get_by_id(id, db).await?))
        } else {
            Ok(None)
        }
    }
}

pub struct OrigEpisode<'a> {
    lang: &'a str,
    episode: &'a str,
}

impl<'a> OrigEpisode<'a> {
    pub fn langname(&self) -> &'a str {
        match self.lang {
            "fr" => "Franska",
            "en" => "Engelska",
            l => l,
        }
    }
}

impl ToHtml for OrigEpisode<'_> {
    fn to_html(&self, out: &mut dyn Write) -> io::Result<()> {
        write!(out, "<q lang='{}'>", self.lang)?;
        self.episode.to_html(out)?;
        out.write_all(b"</q>")
    }
}

pub struct OrigDates {
    from: NaiveDate,
    to: Option<NaiveDate>,
    sun: bool,
}

impl OrigDates {
    pub fn date(date: NaiveDate) -> Self {
        OrigDates {
            from: date,
            to: None,
            sun: false,
        }
    }
    pub fn kind(&self) -> &'static str {
        if self.to.is_none() {
            "Först publicerad"
        } else if self.sun {
            "Söndagssidor"
        } else {
            "Dagstrippar"
        }
    }
}

impl ToHtml for OrigDates {
    fn to_html(&self, out: &mut dyn Write) -> io::Result<()> {
        match (self.from, self.to) {
            (from, Some(to)) if from != to => {
                write!(out, "{} - {}", SvDate(&from), SvDate(&to))
            }
            (date, _) => write!(out, "{}", SvDate(&date)),
        }
    }
}

struct SvDate<'a>(&'a NaiveDate);

impl fmt::Display for SvDate<'_> {
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

static LONG_WEEKDAYS: [&str; 7] = [
    "måndag", "tisdag", "onsdag", "torsdag", "fredag", "lördag", "söndag",
];
static LONG_MONTHS: [&str; 12] = [
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
