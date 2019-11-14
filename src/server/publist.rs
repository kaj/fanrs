use super::sortable_issue;
use crate::models::{
    Creator, CreatorSet, Episode, Issue, IssueRef, PartInIssue, Title,
};
use crate::schema::creator_aliases::dsl as ca;
use crate::schema::episode_parts::dsl as ep;
use crate::schema::episodes::dsl as e;
use crate::schema::episodes_by::dsl as eb;
use crate::schema::issues::dsl as i;
use crate::schema::publications::dsl as p;
use crate::schema::titles::dsl as t;
use crate::templates::ToHtml;
use diesel::dsl::{all, min};
use diesel::pg::PgConnection;
use diesel::prelude::*;
use failure::Error;
use std::collections::BTreeMap;
use std::io::{self, Write};

pub struct PartsPublished {
    issues: Vec<PartInIssue>,
    others: bool,
}

impl PartsPublished {
    pub fn for_episode(
        episode: &Episode,
        db: &PgConnection,
    ) -> Result<PartsPublished, diesel::result::Error> {
        PartsPublished::for_episode_id(episode.id, db)
    }

    pub fn for_episode_id(
        episode: i32,
        db: &PgConnection,
    ) -> Result<PartsPublished, diesel::result::Error> {
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
    fn to_html(&self, out: &mut dyn Write) -> io::Result<()> {
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
    fn to_html(&self, out: &mut dyn Write) -> io::Result<()> {
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

pub struct OtherContribs {
    pub roles: String,
    pub episodes: BTreeMap<Title, Vec<(Option<String>, PartsPublished)>>,
}

impl OtherContribs {
    pub fn for_creator(
        creator: &Creator,
        db: &PgConnection,
    ) -> Result<OtherContribs, diesel::result::Error> {
        let oe_columns = (t::titles::all_columns(), e::id, e::episode);
        let other_episodes = e::episodes
            .inner_join(eb::episodes_by.inner_join(ca::creator_aliases))
            .inner_join(t::titles)
            .filter(ca::creator_id.eq(creator.id))
            .filter(eb::role.ne(all(CreatorSet::MAIN_ROLES)))
            .select(oe_columns)
            .inner_join(
                ep::episode_parts
                    .inner_join(p::publications.inner_join(i::issues)),
            )
            .order(min(sortable_issue()))
            .group_by(oe_columns)
            .load::<(Title, i32, Option<String>)>(db)?;

        let mut oe: BTreeMap<_, Vec<_>> = BTreeMap::new();
        for (title, episode_id, episode) in other_episodes {
            let published = PartsPublished::for_episode_id(episode_id, db)?;
            oe.entry(title).or_default().push((episode, published));
        }

        let o_roles = eb::episodes_by
            .inner_join(ca::creator_aliases)
            .filter(ca::creator_id.eq(creator.id))
            .filter(eb::role.ne(all(CreatorSet::MAIN_ROLES)))
            .select(eb::role)
            .distinct()
            .load::<String>(db)?
            .into_iter()
            .map(|r| match r.as_ref() {
                "color" => "färgläggare",
                "redax" => "redaktion",
                "xlat" => "översättare",
                "textning" => "textsättare",
                _ => "något annat",
            })
            .collect::<Vec<_>>()
            .join(", ");
        Ok(OtherContribs {
            roles: o_roles,
            episodes: oe,
        })
    }

    pub fn is(&self) -> bool {
        !self.episodes.is_empty()
    }
}
