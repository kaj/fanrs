use super::{
    FullArticle, FullEpisode, Paginator, PgFilter, PgPool, RenderRucte,
    Result, ViewError, goh, redirect, wrap,
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
use crate::templates::{self, title_html, titles_html};
use diesel::dsl::{count_distinct, count_star, max, min};
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use serde::Deserialize;
use warp::filters::BoxedFilter;
use warp::http::response::Builder;
use warp::{self, Filter, Reply, reply::Response};

pub fn routes(s: PgFilter) -> BoxedFilter<(impl Reply,)> {
    use warp::filters::query::query;
    use warp::path::{end, param};
    let list = goh().and(end()).and(s.clone()).then(list_titles);
    let one = goh()
        .and(s)
        .and(param())
        .and(end())
        .and(query())
        .then(one_title);
    list.or(one).unify().map(wrap).boxed()
}

async fn list_titles(db: PgPool) -> Result<Response> {
    let mut db = db.get().await?;
    let all = t::titles
        .inner_join(
            e::episodes.inner_join(
                ep::episode_parts
                    .inner_join(p::publications.inner_join(i::issues)),
            ),
        )
        .group_by(t::titles::all_columns())
        .select((
            t::titles::all_columns(),
            count_distinct(e::id),
            min(i::magic),
            max(i::magic),
        ))
        .order(t::title)
        .load::<(_, _, Option<IssueRef>, Option<IssueRef>)>(&mut db)
        .await?
        .into_iter()
        .map(|(title, c, first, last)| {
            // Inner joins, so first/last will not be null.
            (title, c, first.unwrap(), last.unwrap())
        })
        .collect::<Vec<_>>();
    Ok(Builder::new().html(|o| titles_html(o, &all))?)
}

#[derive(Deserialize)]
pub struct PageParam {
    p: Option<usize>,
}

async fn one_title(
    db: PgPool,
    slug: String,
    page: PageParam,
) -> Result<Response> {
    let mut db = db.get().await?;
    let (slug, strip) = if let Some(strip) = slug.strip_prefix("weekdays-") {
        (strip.to_string(), Some(false))
    } else if let Some(strip) = slug.strip_prefix("sundays-") {
        (strip.to_string(), Some(true))
    } else {
        (slug, None)
    };
    let title = t::titles
        .filter(t::slug.eq(slug))
        .first::<Title>(&mut db)
        .await
        .optional()?
        .ok_or(ViewError::NotFound)?;

    let articles_raw = a::articles
        .select(a::articles::all_columns())
        .left_join(ar::article_refkeys.left_join(r::refkeys))
        .filter(r::kind.eq(RefKey::TITLE_ID))
        .filter(r::slug.eq(title.slug.clone()))
        .inner_join(p::publications.inner_join(i::issues))
        .order(min(i::magic))
        .group_by(a::articles::all_columns())
        .load::<Article>(&mut db)
        .await?;
    let mut articles = Vec::with_capacity(articles_raw.len());
    for article in articles_raw {
        let published = i::issues
            .inner_join(p::publications)
            .select((i::year, (i::number, i::number_str)))
            .filter(p::article_id.eq(article.id))
            .load::<IssueRef>(&mut db)
            .await?;
        articles
            .push((FullArticle::load(article, &mut db).await?, published));
    }

    let episodes = e::episodes
        .filter(e::title_id.eq(title.id))
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
                .load::<Episode>(&mut db)
                .await?
        }
        None => {
            episodes
                .order(min(i::magic))
                .load::<Episode>(&mut db)
                .await?
        }
    };

    let (episodes_raw, pages) = Paginator::if_needed(episodes, page.p)
        .map_err(|()| ViewError::NotFound)?;

    let mut episodes = Vec::with_capacity(episodes_raw.len());
    for episode in episodes_raw {
        episodes.push(FullEpisode::load_details(episode, &mut db).await?);
    }

    Ok(Builder::new().html(|o| {
        title_html(o, &title, pages.as_ref(), &articles, &episodes)
    })?)
}

pub async fn oldslug(slug: String, db: PgPool) -> Result<impl Reply> {
    // Special case:
    if slug == "favicon.ico" {
        use templates::statics::goda_svg;
        return redirect(&format!("/s/{}", goda_svg.name));
    }
    if slug == "apple-touch-icon.png"
        || slug == "apple-touch-icon-precomposed.png"
    {
        use templates::statics::sc_png;
        return redirect(&format!("/s/{}", sc_png.name));
    }
    let target = slug.replace('_', "-").replace(".html", "");

    let mut db = db.get().await?;
    if let Ok(year) = target.parse::<i16>() {
        let issues = i::issues
            .filter(i::year.eq(year))
            .select(count_star())
            .first::<i64>(&mut db)
            .await?;
        if issues > 0 {
            return redirect(&format!("/{year}"));
        }
    }
    let target = slug.replace("weekdays-", "").replace("sundays-", "");

    let n = t::titles
        .filter(t::slug.eq(target.clone()))
        .select(count_star())
        .first::<i64>(&mut db)
        .await?;
    if n == 1 {
        return redirect(&format!("/titles/{target}"));
    }
    let target = t::titles
        .filter(
            t::slug.ilike(
                target
                    .replace('-', "")
                    .chars()
                    .map(|c| c.to_string())
                    .collect::<Vec<_>>()
                    .join("%"),
            ),
        )
        .select(t::slug)
        .first::<String>(&mut db)
        .await
        .optional()?
        .ok_or(ViewError::NotFound)?;
    redirect(&format!("/titles/{target}"))
}
