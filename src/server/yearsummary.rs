use super::{custom, PgPool, YearLinks};
use crate::models::{Creator, Issue};
use crate::schema::articles::dsl as a;
use crate::schema::episode_parts::dsl as ep;
use crate::schema::episodes::dsl as e;
use crate::schema::issues::dsl as i;
use crate::schema::publications::dsl as p;
use crate::schema::titles::dsl as t;
use crate::templates::{self, RenderRucte, ToHtml};
use diesel::prelude::*;
use diesel::QueryDsl;
use futures::stream::{self, StreamExt, TryStreamExt};
use std::io::{self, Write};
use tokio_diesel::AsyncRunQueryDsl;
use warp::http::response::Builder;
use warp::{self, reject::not_found, Rejection, Reply};

pub async fn year_summary(
    db: PgPool,
    year: u16,
) -> Result<impl Reply, Rejection> {
    let issues: Vec<Issue> = i::issues
        .filter(i::year.eq(year as i16))
        .order(i::number)
        .load_async(&db)
        .await
        .map_err(custom)?;
    if issues.is_empty() {
        return Err(not_found());
    }
    let issues = stream::iter(issues)
        .then(|issue| load_summary(issue, &db))
        .try_collect::<Vec<_>>()
        .await?;

    let years = YearLinks::load(year, db).await?;
    Builder::new().html(|o| templates::year_summary(o, year, &years, &issues))
}

async fn load_summary(
    issue: Issue,
    db: &PgPool,
) -> Result<(Issue, Vec<Creator>, Vec<ContentSummary>), Rejection> {
    let cover_by = super::cover_by(issue.id, &db).await.map_err(custom)?;

    let contents = p::publications
        .left_outer_join(
            ep::episode_parts.inner_join(e::episodes.inner_join(t::titles)),
        )
        .left_outer_join(a::articles)
        .select((
            (t::slug, t::title, e::episode, ep::part_no, ep::part_name)
                .nullable(),
            (a::title, a::subtitle).nullable(),
        ))
        .filter(p::issue.eq(issue.id))
        .order(p::seqno)
        .load_async::<(Option<ComicSummary>, Option<ArticleSummary>)>(&db)
        .await
        .map_err(custom)?
        .into_iter()
        .map(|i| match i {
            (Some(c), None) => ContentSummary::Comic(c),
            (None, Some(a)) => ContentSummary::Text(a),
            _ => unreachable!(),
        })
        .collect::<Vec<_>>();
    Ok((issue, cover_by, contents))
}

pub enum ContentSummary {
    Comic(ComicSummary),
    Text(ArticleSummary),
}

impl ToHtml for ContentSummary {
    fn to_html(&self, out: &mut dyn Write) -> io::Result<()> {
        match self {
            ContentSummary::Comic(c) => c.to_html(out),
            ContentSummary::Text(a) => a.to_html(out),
        }
    }
}

#[derive(Debug, Queryable)]
pub struct ComicSummary {
    slug: String,
    title: String,
    episode: Option<String>,
    part_no: Option<i16>,
    part_name: Option<String>,
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
        if self.part_no.is_some() || self.part_name.is_some() {
            out.write_all(b" <span class='part'>")?;
            if let Some(no) = self.part_no {
                write!(out, "del {}", no)?;
                if self.part_name.is_some() {
                    out.write_all(b": ")?;
                }
            }
            if let Some(ref name) = &self.part_name {
                name.to_html(out)?;
            }
            out.write_all(b"</span>")?;
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
