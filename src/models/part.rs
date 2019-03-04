use super::{Episode, Issue, IssueRef};
use crate::templates::ToHtml;
use diesel::pg::PgConnection;
use diesel::prelude::*;
use diesel::result::Error;
use std::io::{self, Write};

#[derive(Debug, Queryable)]
pub struct Part {
    pub id: i32,
    pub no: Option<i16>,
    pub name: Option<String>,
}

impl Part {
    pub fn publish(
        episode: &Episode,
        part_no: Option<i16>,
        part_name: Option<&str>,
        issue: &Issue,
        seqno: Option<i16>,
        best_plac: Option<i16>,
        label: &str,
        db: &PgConnection,
    ) -> Result<(), Error> {
        use crate::schema::episode_parts::dsl as e;
        //let part_no = part.and_then(|p| p.no.map(i16::from));
        //let part_name = part.and_then(|p| p.name.as_ref());
        let epq = e::episode_parts
            .select(e::id)
            .filter(e::episode.eq(episode.id))
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
                    e::episode.eq(episode.id),
                    e::part_no.eq(part_no),
                    e::part_name.eq(part_name),
                ))
                .get_result::<(i32, i32, Option<i16>, Option<String>)>(db)?
                .0
        };
        use crate::schema::publications::dsl as p;
        if let Some((id, old_seqno, old_label)) = p::publications
            .filter(p::issue.eq(issue.id))
            .filter(p::episode_part.eq(part_id))
            .select((p::id, p::seqno, p::label))
            .first::<(i32, Option<i16>, String)>(db)
            .optional()?
        {
            if seqno.is_some() && old_seqno != seqno {
                eprintln!("TODO: Should update seqno for {}", id);
            }
            if label != "" && old_label != label {
                diesel::update(p::publications)
                    .set(p::label.eq(label))
                    .filter(p::id.eq(id))
                    .execute(db)?;
            }
            Ok(())
        } else {
            diesel::insert_into(p::publications)
                .values((
                    p::issue.eq(issue.id),
                    p::episode_part.eq(part_id),
                    p::seqno.eq(seqno),
                    p::best_plac.eq(best_plac),
                    p::label.eq(label),
                ))
                .execute(db)?;
            Ok(())
        }
    }

    fn is_some(&self) -> bool {
        self.no.is_some() || self.name.is_some()
    }
    pub fn is_first(&self) -> bool {
        self.no.map(|n| n == 1).unwrap_or(true)
    }
}

impl ToHtml for Part {
    fn to_html(&self, out: &mut Write) -> io::Result<()> {
        if !(self.no.is_some() || self.name.is_some()) {
            return Ok(());
        }
        write!(out, "<span class='part'>")?;
        if let Some(no) = self.no {
            write!(out, "del {}", no)?;
            if self.name.is_some() {
                write!(out, ": ")?;
            }
        }
        if let Some(ref name) = self.name {
            write!(out, "{}", name)?;
        }
        write!(out, "</span>")
    }
}

#[derive(Debug, Queryable)]
pub struct PartInIssue(IssueRef, Part);

impl ToHtml for PartInIssue {
    fn to_html(&self, out: &mut Write) -> io::Result<()> {
        self.0.to_html(out)?;
        if self.1.is_some() {
            write!(out, " (")?;
            self.1.to_html(out)?;
            write!(out, ")")?;
        }
        Ok(())
    }
}
