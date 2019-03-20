use super::PooledPg;
use super::{custom, custom_or_404, sortable_issue};
use super::{named, redirect, FullEpisode, Paginator, RenderRucte};
use crate::models::{
    Article, CreatorSet, Episode, IssueRef, RefKey, RefKeySet, Title,
};
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
use diesel::pg::PgConnection;
use diesel::prelude::*;
use diesel::sql_types::SmallInt;
use failure::Error;
use warp::http::Response;
use warp::reject::not_found;
use warp::{self, Rejection, Reply};

pub fn title_cloud(
    num: i64,
    db: &PgConnection,
) -> Result<Vec<(Title, i64, u8)>, Error> {
    let (c_def, c) = named(sql("count(*)"), "c");
    let mut titles = t::titles
        .left_join(e::episodes.left_join(ep::episode_parts))
        .select((t::titles::all_columns(), c_def))
        .group_by(t::titles::all_columns())
        .order(c.desc())
        .limit(num)
        .load::<(Title, i64)>(db)?
        .into_iter()
        .enumerate()
        .map(|(n, (title, c))| (title, c, (8 * (num - n as i64) / num) as u8))
        .collect::<Vec<_>>();
    titles.sort_by(|a, b| a.0.title.cmp(&b.0.title));
    Ok(titles)
}

#[allow(clippy::needless_pass_by_value)]
pub fn list_titles(db: PooledPg) -> Result<impl Reply, Rejection> {
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
    Response::builder().html(|o| templates::titles(o, &all))
}

#[derive(Deserialize)]
pub struct PageParam {
    p: Option<usize>,
}

#[allow(clippy::needless_pass_by_value)]
pub fn one_title(
    db: PooledPg,
    slug: String,
    page: PageParam,
) -> Result<impl Reply, Rejection> {
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
            let refs = RefKeySet::for_article(&article, &db).unwrap();
            let creators = CreatorSet::for_article(&article, &db).unwrap();
            let published = i::issues
                .inner_join(p::publications)
                .select((i::year, (i::number, i::number_str)))
                .filter(p::article_id.eq(article.id))
                .load::<IssueRef>(&db)
                .unwrap();
            (article, refs, creators, published)
        })
        .collect::<Vec<_>>();

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

    Response::builder().html(|o| {
        templates::title(o, &title, pages.as_ref(), &articles, &episodes)
    })
}

pub fn oldslug_title(
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
