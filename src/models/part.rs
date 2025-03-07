use super::{Episode, Issue, IssueRef};
use crate::schema::episode_parts::dsl as ep;
use crate::schema::publications::dsl as p;
use crate::templates::ToHtml;
use diesel::dsl::count_star;
use diesel::prelude::*;
use diesel::result::Error;
use diesel_async::{AsyncPgConnection, RunQueryDsl};
use std::io::{self, Write};
use tracing::warn;

#[derive(Debug, Queryable)]
pub struct Part {
    pub no: Option<i16>,
    pub name: Option<String>,
}

impl Part {
    fn none() -> Part {
        Part {
            no: None,
            name: None,
        }
    }
    /// true for an actual part, false for the whole episode
    pub fn is_part(&self) -> bool {
        self.no.is_some() || self.name.is_some()
    }
    pub fn is_first(&self) -> bool {
        self.no.is_none_or(|n| n == 1)
    }
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    pub async fn publish(
        episode: &Episode,
        part: &Part,
        issue: &Issue,
        seqno: Option<i16>,
        best_plac: Option<i16>,
        label: &str,
        db: &mut AsyncPgConnection,
    ) -> Result<(), Error> {
        let mut existing = p::publications
            .select(count_star())
            .left_join(ep::episode_parts)
            .filter(ep::episode_id.eq(episode.id))
            .filter(p::issue_id.eq(issue.id))
            .into_boxed();
        if part.is_part() {
            existing = existing
                .filter(ep::part_no.is_not_distinct_from(part.no))
                .filter(ep::part_name.is_not_distinct_from(part.name()));
        }
        match existing.first::<i64>(db).await? {
            0 => (),
            1 => return Ok(()),
            n => warn!("{} of {:?} in {}", n, episode, issue),
        }
        let part_id = Self::g_o_c_part_id(episode.id, part, db).await?;
        if let Some((id, old_seqno, old_label)) = p::publications
            .filter(p::issue_id.eq(issue.id))
            .filter(p::episode_part.eq(part_id))
            .select((p::id, p::seqno, p::label))
            .first::<(i32, Option<i16>, String)>(db)
            .await
            .optional()?
        {
            if seqno.is_some() && old_seqno != seqno {
                unimplemented!(
                    "Should update seqno for publication #{} ({:?} != {:?})",
                    id,
                    seqno,
                    old_seqno
                );
            }
            if !label.is_empty() && old_label != label {
                diesel::update(p::publications)
                    .set(p::label.eq(label))
                    .filter(p::id.eq(id))
                    .execute(db)
                    .await?;
            }
        } else {
            diesel::insert_into(p::publications)
                .values((
                    p::issue_id.eq(issue.id),
                    p::episode_part.eq(part_id),
                    p::seqno.eq(seqno),
                    p::best_plac.eq(best_plac),
                    p::label.eq(label),
                ))
                .execute(db)
                .await?;
        }
        Ok(())
    }
    pub async fn prevpub(
        episode: &Episode,
        issue: &Issue,
        db: &mut AsyncPgConnection,
    ) -> Result<(), Error> {
        let existing = p::publications
            .select(count_star())
            .left_join(ep::episode_parts)
            .filter(ep::episode_id.eq(episode.id))
            .filter(p::issue_id.eq(issue.id));
        if existing.first::<i64>(db).await? > 0 {
            return Ok(());
        }
        let part_id =
            Self::g_o_c_part_id(episode.id, &Part::none(), db).await?;
        diesel::insert_into(p::publications)
            .values((p::issue_id.eq(issue.id), p::episode_part.eq(part_id)))
            .execute(db)
            .await?;
        Ok(())
    }

    async fn g_o_c_part_id(
        episode_id: i32,
        part: &Part,
        db: &mut AsyncPgConnection,
    ) -> Result<i32, Error> {
        if let Some(part_id) = ep::episode_parts
            .select(ep::id)
            .filter(ep::episode_id.eq(episode_id))
            .filter(ep::part_no.is_not_distinct_from(part.no))
            .filter(ep::part_name.is_not_distinct_from(part.name()))
            .first::<i32>(db)
            .await
            .optional()?
        {
            Ok(part_id)
        } else {
            Ok(diesel::insert_into(ep::episode_parts)
                .values((
                    ep::episode_id.eq(episode_id),
                    ep::part_no.eq(part.no),
                    ep::part_name.eq(part.name()),
                ))
                .returning(ep::id)
                .get_result(db)
                .await?)
        }
    }
}

impl ToHtml for Part {
    fn to_html(&self, out: &mut dyn Write) -> io::Result<()> {
        if !self.is_part() {
            return Ok(());
        }
        write!(out, "<span class='part'>")?;
        if let Some(no) = self.no {
            write!(out, "del {no}")?;
            if self.name.is_some() {
                write!(out, ": ")?;
            }
        }
        if let Some(ref name) = self.name {
            name.to_html(out)?;
        }
        write!(out, "</span>")
    }
}

#[derive(Debug, Queryable)]
pub struct PartInIssue(pub IssueRef, pub Part, pub Option<i16>);

impl ToHtml for PartInIssue {
    fn to_html(&self, out: &mut dyn Write) -> io::Result<()> {
        self.0.to_html(out)?;
        if self.1.is_part() {
            write!(out, " (")?;
            self.1.to_html(out)?;
            write!(out, ")")?;
        }
        Ok(())
    }
}
