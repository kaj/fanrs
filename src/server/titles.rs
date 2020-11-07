use super::{
    goh, redirect, FullArticle, FullEpisode, OptionalExtension, Paginator,
    PgFilter, PgPool, RenderRucte, ServerError,
};
use crate::models::{Article, Episode, IssueRef, RefKey, Title};
use crate::schema::article_refkeys::dsl as ar;
use crate::schema::articles::dsl as a;
use crate::schema::episode_parts::dsl as ep;
use crate::schema::episodes::dsl as e;
use crate::schema::issues::dsl as i;
use crate::schema::publications::dsl as p;
use crate::schema::refkeys::dsl as r;
use crate::schema::titles::dsl as t;
use crate::templates;
use diesel::dsl::{count_star, min, sql};
use diesel::prelude::*;
use diesel::sql_types::SmallInt;
use serde::Deserialize;
use tokio_diesel::AsyncRunQueryDsl;
use warp::filters::BoxedFilter;
use warp::http::response::Builder;
use warp::reject::not_found;
use warp::{self, reply::Response, Filter, Reply};

pub fn routes(s: PgFilter) -> BoxedFilter<(impl Reply,)> {
    use warp::filters::query::query;
    use warp::path::{end, param};
    let s = || s.clone();
    let list = goh().and(end()).and(s()).map_async(list_titles);
    let one = goh()
        .and(s())
        .and(param())
        .and(end())
        .and(query())
        .map_async(one_title);
    list.or(one).unify().boxed()
}

#[allow(clippy::needless_pass_by_value)]
async fn list_titles(db: PgPool) -> Result<Response, ServerError> {
    let all = t::titles
        .left_join(e::episodes.left_join(
            ep::episode_parts.left_join(p::publications.left_join(i::issues)),
        ))
        .select((
            t::titles::all_columns(),
            sql("count(*)"),
            sql::<SmallInt>("min(magic)"),
            sql::<SmallInt>("max(magic)"),
        ))
        .group_by(t::titles::all_columns())
        .order(t::title)
        .load_async(&db)
        .await?
        .into_iter()
        .map(|(title, c, first, last)| {
            Ok((
                title,
                c,
                IssueRef::from_magic(first),
                IssueRef::from_magic(last),
            ))
        })
        .collect::<Result<Vec<_>, ServerError>>()?;
    Ok(Builder::new().html(|o| templates::titles(o, &all))?)
}

#[derive(Deserialize)]
pub struct PageParam {
    p: Option<usize>,
}

#[allow(clippy::needless_pass_by_value)]
async fn one_title(
    db: PgPool,
    slug: String,
    page: PageParam,
) -> Result<Response, ServerError> {
    let (slug, strip) = if let Some(strip) = slug.strip_prefix("weekdays-") {
        (strip.to_string(), Some(false))
    } else if let Some(strip) = slug.strip_prefix("sundays-") {
        (strip.to_string(), Some(true))
    } else {
        (slug, None)
    };
    let title = t::titles
        .filter(t::slug.eq(slug))
        .first_async::<Title>(&db)
        .await
        .or_404()?;

    let articles_raw = a::articles
        .select(a::articles::all_columns())
        .left_join(ar::article_refkeys.left_join(r::refkeys))
        .filter(r::kind.eq(RefKey::TITLE_ID))
        .filter(r::slug.eq(title.slug.clone()))
        .inner_join(p::publications.inner_join(i::issues))
        .order(min(i::magic))
        .group_by(a::articles::all_columns())
        .load_async::<Article>(&db)
        .await
        .or_404()?;
    let mut articles = Vec::with_capacity(articles_raw.len());
    for article in articles_raw.into_iter() {
        let published = i::issues
            .inner_join(p::publications)
            .select((i::year, (i::number, i::number_str)))
            .filter(p::article_id.eq(article.id))
            .load_async::<IssueRef>(&db)
            .await?;
        articles
            .push((FullArticle::load_async(article, &db).await?, published));
    }

    let episodes = e::episodes
        .filter(e::title.eq(title.id))
        .select(e::episodes::all_columns())
        .inner_join(
            ep::episode_parts
                .inner_join(p::publications.inner_join(i::issues)),
        )
        .group_by(crate::schema::episodes::all_columns);

    let episodes = match strip {
        Some(sun) => {
            episodes
                .filter(e::orig_sundays.eq(sun))
                .filter(e::orig_date.is_not_null())
                .order(e::orig_date)
                .load_async::<Episode>(&db)
                .await?
        }
        None => {
            episodes
                .order(min(i::magic))
                .load_async::<Episode>(&db)
                .await?
        }
    };

    let (episodes_raw, pages) =
        Paginator::if_needed(episodes, page.p).map_err(|()| not_found())?;

    let mut episodes = Vec::with_capacity(episodes_raw.len());
    for episode in episodes_raw.into_iter() {
        episodes.push(FullEpisode::load_details_async(episode, &db).await?);
    }

    Ok(Builder::new().html(|o| {
        templates::title(o, &title, pages.as_ref(), &articles, &episodes)
    })?)
}

pub async fn oldslug(
    db: PgPool,
    slug: String,
) -> Result<impl Reply, ServerError> {
    // Special case:
    if slug == "favicon.ico" {
        use templates::statics::goda_svg;
        return Ok(redirect(&format!("/s/{}", goda_svg.name)));
    }
    if slug == "apple-touch-icon.png"
        || slug == "apple-touch-icon-precomposed.png"
    {
        use templates::statics::sc_png;
        return Ok(redirect(&format!("/s/{}", sc_png.name)));
    }
    let target = slug.replace("_", "-").replace(".html", "");

    if let Ok(year) = target.parse::<i16>() {
        let issues = i::issues
            .filter(i::year.eq(year))
            .select(count_star())
            .first_async::<i64>(&db)
            .await?;
        if issues > 0 {
            return Ok(redirect(&format!("/{}", year)));
        }
    }
    let target = slug.replace("weekdays-", "").replace("sundays-", "");

    let n = t::titles
        .filter(t::slug.eq(target.clone()))
        .select(count_star())
        .first_async::<i64>(&db)
        .await?;
    if n == 1 {
        return Ok(redirect(&format!("/titles/{}", target)));
    }
    let target = t::titles
        .filter(
            t::slug.ilike(
                target
                    .replace("-", "")
                    .chars()
                    .map(|c| c.to_string())
                    .collect::<Vec<_>>()
                    .join("%"),
            ),
        )
        .select(t::slug)
        .first_async::<String>(&db)
        .await
        .or_404()?;
    Ok(redirect(&format!("/titles/{}", target)))
}
