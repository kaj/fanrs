use super::{
    custom, custom_or_404, goh, redirect, sortable_issue, FullArticle,
    FullEpisode, Paginator, PgFilter, PooledPg, RenderRucte,
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
use warp::filters::BoxedFilter;
use warp::http::response::Builder;
use warp::reject::not_found;
use warp::{self, reply::Response, Filter, Rejection, Reply};

pub fn routes(s: PgFilter) -> BoxedFilter<(impl Reply,)> {
    use warp::filters::query::query;
    use warp::path::{end, param};
    let s = || s.clone();
    let list = goh().and(end()).and(s()).and_then(list_titles);
    let one = goh()
        .and(s())
        .and(param())
        .and(end())
        .and(query())
        .and_then(one_title);
    list.or(one).unify().boxed()
}

#[allow(clippy::needless_pass_by_value)]
async fn list_titles(db: PooledPg) -> Result<Response, Rejection> {
    let all = t::titles
        .left_join(e::episodes.left_join(
            ep::episode_parts.left_join(p::publications.left_join(i::issues)),
        ))
        .select((
            t::titles::all_columns(),
            sql("count(*)"),
            sql::<SmallInt>(&format!("min({})", IssueRef::MAGIC_Q)),
            sql::<SmallInt>(&format!("max({})", IssueRef::MAGIC_Q)),
        ))
        .group_by(t::titles::all_columns())
        .order(t::title)
        .load(&db)
        .map_err(custom)?
        .into_iter()
        .map(|(title, c, first, last)| {
            Ok((
                title,
                c,
                IssueRef::from_magic(first),
                IssueRef::from_magic(last),
            ))
        })
        .collect::<Result<Vec<_>, Rejection>>()?;
    Builder::new().html(|o| templates::titles(o, &all))
}

#[derive(Deserialize)]
pub struct PageParam {
    p: Option<usize>,
}

#[allow(clippy::needless_pass_by_value)]
async fn one_title(
    db: PooledPg,
    slug: String,
    page: PageParam,
) -> Result<Response, Rejection> {
    let (slug, strip) = if slug.starts_with("weekdays-") {
        (&slug["weekdays-".len()..], Some(false))
    } else if slug.starts_with("sundays-") {
        (&slug["sundays-".len()..], Some(true))
    } else {
        (slug.as_ref(), None)
    };
    let title = t::titles
        .filter(t::slug.eq(slug))
        .first::<Title>(&db)
        .map_err(custom_or_404)?;

    let articles = a::articles
        .select(a::articles::all_columns())
        .left_join(ar::article_refkeys.left_join(r::refkeys))
        .filter(r::kind.eq(RefKey::TITLE_ID))
        .filter(r::slug.eq(&title.slug))
        .inner_join(p::publications.inner_join(i::issues))
        .order(min(sortable_issue()))
        .group_by(a::articles::all_columns())
        .load::<Article>(&db)
        .map_err(custom)?
        .into_iter()
        .map(|article| {
            let published = i::issues
                .inner_join(p::publications)
                .select((i::year, (i::number, i::number_str)))
                .filter(p::article_id.eq(article.id))
                .load::<IssueRef>(&db)?;
            Ok((FullArticle::load(article, &db)?, published))
        })
        .collect::<Result<Vec<_>, failure::Error>>()
        .map_err(custom)?;

    let episodes = e::episodes
        .filter(e::title.eq(title.id))
        .select(e::episodes::all_columns())
        .inner_join(
            ep::episode_parts
                .inner_join(p::publications.inner_join(i::issues)),
        )
        .group_by(crate::schema::episodes::all_columns)
        .into_boxed();
    let episodes = match strip {
        Some(sun) => episodes
            .filter(e::orig_sundays.eq(sun))
            .filter(e::orig_date.is_not_null())
            .order(e::orig_date),
        None => {
            episodes.order(min(sql::<SmallInt>("(year-1950)*64 + number")))
        }
    };
    let episodes = episodes.load::<Episode>(&db).map_err(custom)?;

    let (episodes, pages) =
        Paginator::if_needed(episodes, page.p).map_err(|()| not_found())?;

    let episodes = episodes
        .into_iter()
        .map(|episode| FullEpisode::load_details(episode, &db))
        .collect::<Result<Vec<_>, _>>()
        .map_err(custom)?;

    Builder::new().html(|o| {
        templates::title(o, &title, pages.as_ref(), &articles, &episodes)
    })
}

pub async fn oldslug(
    db: PooledPg,
    slug: String,
) -> Result<impl Reply, Rejection> {
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
    let target = slug.replace("_", "-").replace(".html", "");

    if let Ok(year) = target.parse::<i16>() {
        let issues = i::issues
            .filter(i::year.eq(year))
            .select(count_star())
            .first::<i64>(&db)
            .map_err(custom)?;
        if issues > 0 {
            return redirect(&format!("/{}", year));
        }
    }
    let target = slug.replace("weekdays-", "").replace("sundays-", "");

    let n = t::titles
        .filter(t::slug.eq(&target))
        .select(count_star())
        .first::<i64>(&db)
        .map_err(custom)?;
    if n == 1 {
        return redirect(&format!("/titles/{}", target));
    }
    let target = t::titles
        .filter(
            t::slug.ilike(
                &target
                    .replace("-", "")
                    .chars()
                    .map(|c| c.to_string())
                    .collect::<Vec<_>>()
                    .join("%"),
            ),
        )
        .select(t::slug)
        .first::<String>(&db)
        .map_err(custom_or_404)?;
    redirect(&format!("/titles/{}", target))
}
