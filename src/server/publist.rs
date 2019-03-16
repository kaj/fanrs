use crate::models::{Episode, Issue, IssueRef, PartInIssue};
use crate::schema::episode_parts::dsl as ep;
use crate::schema::issues::dsl as i;
use crate::schema::publications::dsl as p;
use crate::templates::ToHtml;
use diesel::pg::PgConnection;
use diesel::prelude::*;
use failure::Error;
use std::io::{self, Write};

pub struct PartsPublished {
    issues: Vec<PartInIssue>,
    others: bool,
}

impl PartsPublished {
    pub fn for_episode(
        episode: &Episode,
        db: &PgConnection,
    ) -> Result<PartsPublished, Error> {
        PartsPublished::for_episode_id(episode.id, db)
    }

    pub fn for_episode_id(
        episode: i32,
        db: &PgConnection,
    ) -> Result<PartsPublished, Error> {
        Ok(PartsPublished {
            issues: i::issues
                .inner_join(p::publications.inner_join(ep::episode_parts))
                .select((
                    (i::year, (i::number, i::number_str)),
                    (ep::id, ep::part_no, ep::part_name),
                ))
                .filter(ep::episode.eq(episode))
                .order((i::year, i::number))
                .load::<PartInIssue>(db)?,
            others: false,
        })
    }
    pub fn for_episode_except(
        episode: &Episode,
        issue: &Issue,
        db: &PgConnection,
    ) -> Result<PartsPublished, Error> {
        Ok(PartsPublished {
            issues: i::issues
                .inner_join(p::publications.inner_join(ep::episode_parts))
                .select((
                    (i::year, (i::number, i::number_str)),
                    (ep::id, ep::part_no, ep::part_name),
                ))
                .filter(ep::episode.eq(episode.id))
                .filter(i::id.ne(issue.id))
                .order((i::year, i::number))
                .load::<PartInIssue>(db)?,
            others: true,
        })
    }
    pub fn small(&self) -> SmallPartsPublished {
        SmallPartsPublished(&self)
    }
    pub fn last(&self) -> Option<&IssueRef> {
        self.issues.last().map(|p| &p.0)
    }
}

pub struct SmallPartsPublished<'a>(&'a PartsPublished);

impl ToHtml for PartsPublished {
    fn to_html(&self, out: &mut Write) -> io::Result<()> {
        if let Some((last, pubs)) = self.issues.split_last() {
            out.write_all(b"<p class='info pub'>")?;
            if self.others {
                out.write_all("Även publicerad i ".as_bytes())?;
            } else {
                out.write_all(b"Publicerad i ")?;
            }
            for p in pubs {
                p.to_html(out)?;
                out.write_all(b", ")?;
            }
            last.to_html(out)?;
            out.write_all(b".</p>")?;
        }
        Ok(())
    }
}

impl<'a> ToHtml for SmallPartsPublished<'a> {
    fn to_html(&self, out: &mut Write) -> io::Result<()> {
        if let Some((last, pubs)) = self.0.issues.split_last() {
            out.write_all(b"<small class='pub'>")?;
            for p in pubs {
                p.to_html(out)?;
                out.write_all(b", ")?;
            }
            last.to_html(out)?;
            out.write_all(b".</small>")?;
        }
        Ok(())
    }
}
