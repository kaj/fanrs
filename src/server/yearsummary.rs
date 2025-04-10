use super::{DbError, PgPool, Result, ViewError, YearLinks};
use crate::models::{Creator, Issue, Part};
use crate::schema::articles::dsl as a;
use crate::schema::episode_parts::dsl as ep;
use crate::schema::episodes::dsl as e;
use crate::schema::issues::dsl as i;
use crate::schema::publications::dsl as p;
use crate::schema::titles::dsl as t;
use crate::templates::{RenderRucte, ToHtml, year_summary_html};
use diesel::{dsl, prelude::*};
use diesel_async::{AsyncPgConnection, RunQueryDsl};
use std::io::{self, Write};
use warp::http::response::Builder;
use warp::{self, Reply};

pub async fn year_summary(year: i16, db: PgPool) -> Result<impl Reply> {
    let mut db = db.get().await?;

    let (ord_min, ord_max): (Option<i32>, Option<i32>) = i::issues
        .filter(i::year.eq(year))
        .select((dsl::min(i::ord), dsl::max(i::ord)))
        .first(&mut db)
        .await?;
    let ord = ord_min.and_then(|min| ord_max.map(|max| (min, max)));

    let issues_in: Vec<Issue> = i::issues
        .filter(i::year.eq(year))
        .order(i::number)
        .load(&mut db)
        .await?;
    if issues_in.is_empty() {
        return Err(ViewError::NotFound);
    }
    let mut issues = Vec::with_capacity(issues_in.len());
    for issue in issues_in {
        issues.push(load_summary(issue, &mut db).await?);
    }
    let years = YearLinks::load(year, &mut db).await?;
    Ok(Builder::new()
        .html(|o| year_summary_html(o, year, ord, &years, &issues))?)
}

async fn load_summary(
    issue: Issue,
    db: &mut AsyncPgConnection,
) -> Result<(Issue, Vec<Creator>, Vec<ContentSummary>), DbError> {
    let cover_by = super::cover_by(issue.id, db).await?;

    let contents = p::publications
        .left_outer_join(
            ep::episode_parts.inner_join(e::episodes.inner_join(t::titles)),
        )
        .left_outer_join(a::articles)
        .select((
            (t::slug, t::title, e::name, (ep::part_no, ep::part_name))
                .nullable(),
            (a::title, a::subtitle).nullable(),
            p::best_plac,
        ))
        .filter(p::issue_id.eq(issue.id))
        .order(p::seqno)
        .load(db)
        .await?
        .into_iter()
        .map(|i| match i {
            (Some(c), None, plac) => ContentSummary::Comic(c, plac),
            (None, Some(a), _) => ContentSummary::Text(a),
            _ => unreachable!(),
        })
        .collect::<Vec<_>>();
    Ok((issue, cover_by, contents))
}

pub enum ContentSummary {
    Comic(ComicSummary, Option<i16>),
    Text(ArticleSummary),
}
impl ContentSummary {
    pub fn get_class(&self) -> String {
        match self {
            ContentSummary::Comic(_, plac) => {
                if let Some(plac) = plac.filter(|p| *p <= 3) {
                    format!("comic best{plac}")
                } else {
                    "comic".into()
                }
            }
            ContentSummary::Text(_) => String::new(),
        }
    }
}

impl ToHtml for ContentSummary {
    fn to_html(&self, out: &mut dyn Write) -> io::Result<()> {
        match self {
            ContentSummary::Comic(c, _plac) => c.to_html(out),
            ContentSummary::Text(a) => a.to_html(out),
        }
    }
}

#[derive(Debug, Queryable)]
pub struct ComicSummary {
    slug: String,
    title: String,
    episode: Option<String>,
    part: Part,
}

// <strong><a href="/titles/slug">title</a>[episode]</strong> [part]
impl ToHtml for ComicSummary {
    fn to_html(&self, out: &mut dyn Write) -> io::Result<()> {
        write!(out, "<strong><a href='/titles/{}'>", self.slug)?;
        self.title.to_html(out)?;
        out.write_all(b"</a>")?;
        if let Some(episode) = &self.episode {
            out.write_all(b": ")?;
            episode.to_html(out)?;
        }
        out.write_all(b"</strong>")?;
        if self.part.is_part() {
            out.write_all(b" ")?;
            self.part.to_html(out)?;
        }
        Ok(())
    }
}

#[derive(Debug, Queryable)]
pub struct ArticleSummary {
    title: String,
    subtitle: Option<String>,
}
// @article.title@if let Some(ref s) = article.subtitle {: @s}}
impl ToHtml for ArticleSummary {
    fn to_html(&self, out: &mut dyn Write) -> io::Result<()> {
        self.title.to_html(out)?;
        if let Some(subtitle) = &self.subtitle {
            out.write_all(b": ")?;
            subtitle.to_html(out)?;
        }
        Ok(())
    }
}
