use super::{
    goh, redirect, FullArticle, FullEpisode, PgFilter, PgPool, ServerError,
};
use crate::models::{Article, Episode, IdRefKey, IssueRef, RefKey, Title};
use crate::schema::article_refkeys::dsl as ar;
use crate::schema::articles::dsl as a;
use crate::schema::episode_parts::dsl as ep;
use crate::schema::episode_refkeys::dsl as er;
use crate::schema::episodes::dsl as e;
use crate::schema::issues::dsl as i;
use crate::schema::publications::dsl as p;
use crate::schema::refkeys::dsl as r;
use crate::schema::titles::dsl as t;
use crate::templates::{self, RenderRucte};
use diesel::dsl::{count_star, min, sql};
use diesel::prelude::*;
use diesel::sql_types::{Integer, SmallInt};
use diesel::QueryDsl;
use tokio_diesel::{AsyncError, AsyncRunQueryDsl, OptionalExtension};
use warp::filters::BoxedFilter;
use warp::http::Response;
use warp::{self, Filter, Reply};

type ByteResponse = warp::reply::Response;

pub fn what_routes(s: PgFilter) -> BoxedFilter<(impl Reply,)> {
    use warp::path::{end, param};
    let list = goh().and(end()).and(s.clone()).map_async(list_refs);
    let one = goh().and(s).and(param()).and(end()).map_async(one_ref);
    list.or(one).unify().boxed()
}

pub async fn get_all_fa(db: &PgPool) -> Result<Vec<RefKey>, AsyncError> {
    Ok(r::refkeys
        .filter(r::kind.eq(RefKey::FA_ID))
        .order((sql::<Integer>("cast(substr(slug, 1, 2) as int)"), r::slug))
        .load_async::<IdRefKey>(db)
        .await?
        .into_iter()
        .map(|rk| rk.refkey)
        .collect())
}

#[allow(clippy::needless_pass_by_value)]
async fn list_refs(db: PgPool) -> Result<ByteResponse, ServerError> {
    let all = r::refkeys
        .filter(r::kind.eq(RefKey::KEY_ID))
        .left_join(er::episode_refkeys.left_join(e::episodes.left_join(
            ep::episode_parts.left_join(p::publications.left_join(i::issues)),
        )))
        .select((
            r::refkeys::all_columns(),
            sql("count(*)"),
            sql::<SmallInt>("min(magic)").nullable(),
            sql::<SmallInt>("max(magic)").nullable(),
        ))
        .group_by(r::refkeys::all_columns())
        .order(r::title)
        .load_async::<(IdRefKey, i64, Option<i16>, Option<i16>)>(&db)
        .await?
        .into_iter()
        .map(|(refkey, c, first, last)| {
            (
                refkey.refkey,
                c,
                first.map(IssueRef::from_magic),
                last.map(IssueRef::from_magic),
            )
        })
        .collect::<Vec<_>>();
    Ok(Response::builder().html(|o| templates::refkeys(o, &all))?)
}

pub async fn one_fa(
    db: PgPool,
    slug: String,
) -> Result<ByteResponse, ServerError> {
    one_ref_impl(db, slug, RefKey::FA_ID).await
}

async fn one_ref(
    db: PgPool,
    slug: String,
) -> Result<ByteResponse, ServerError> {
    one_ref_impl(db, slug, RefKey::KEY_ID).await
}

async fn one_ref_impl(
    db: PgPool,
    slug: String,
    kind: i16,
) -> Result<ByteResponse, ServerError> {
    let refkey = r::refkeys
        .filter(r::kind.eq(kind))
        .filter(r::slug.eq(slug.clone()))
        .first_async::<IdRefKey>(&db)
        .await
        .optional()?;
    let refkey = if let Some(refkey) = refkey {
        refkey
    } else {
        if kind == RefKey::FA_ID {
            // Some special cases
            if slug == "17.1" {
                return Ok(redirect("/fa/17j"));
            } else if slug == "22.1" {
                return Ok(redirect("/fa/22k"));
            } else if slug == "22.2" {
                return Ok(redirect("/fa/22j"));
            }
        }
        if kind == RefKey::KEY_ID {
            if slug == "christophe_derrant" {
                return Ok(redirect("/what/christophe-d-errant"));
            } else if slug == "olangofolket" {
                return Ok(redirect("/what/olango-folket"));
            } else if slug == "/what/piratpete" {
                return Ok(redirect("/what/pirat-pete"));
            }
        }
        let target =
            slug.to_lowercase().replace("_", "-").replace(".html", "");
        if target != slug {
            log::debug!("Trying refkey redirect {:?} -> {:?}", slug, target);
            let n = r::refkeys
                .filter(r::kind.eq(kind))
                .filter(r::slug.eq(target.clone()))
                .select(count_star())
                .first_async::<i64>(&db)
                .await?;
            if n == 1 {
                return Ok(redirect(&format!(
                    "/{}/{}",
                    if kind == RefKey::FA_ID { "fa" } else { "what" },
                    target,
                )));
            }
        }
        return Err(ServerError::not_found());
    };

    let raw_articles = a::articles
        .select(a::articles::all_columns())
        .left_join(ar::article_refkeys.left_join(r::refkeys))
        .filter(ar::refkey_id.eq(refkey.id))
        .inner_join(p::publications.inner_join(i::issues))
        .order(min(i::magic))
        .group_by(a::articles::all_columns())
        .load_async::<Article>(&db)
        .await?;

    let mut articles = Vec::with_capacity(raw_articles.len());
    for article in raw_articles.into_iter() {
        let published = i::issues
            .inner_join(p::publications)
            .select((i::year, (i::number, i::number_str)))
            .filter(p::article_id.eq(article.id))
            .load_async::<IssueRef>(&db)
            .await?;
        articles
            .push((FullArticle::load_async(article, &db).await?, published));
    }

    let raw_episodes = e::episodes
        .left_join(er::episode_refkeys)
        .inner_join(t::titles)
        .filter(er::refkey_id.eq(refkey.id))
        .select((t::titles::all_columns(), e::episodes::all_columns()))
        .inner_join(
            ep::episode_parts
                .inner_join(p::publications.inner_join(i::issues)),
        )
        .order(min(i::magic))
        .group_by((t::titles::all_columns(), e::episodes::all_columns()))
        .load_async::<(Title, Episode)>(&db)
        .await?;
    let mut episodes = Vec::with_capacity(raw_episodes.len());
    for (t, ep) in raw_episodes.into_iter() {
        let e = FullEpisode::load_details_async(ep, &db).await?;
        episodes.push((t, e));
    }

    Ok(Response::builder()
        .html(|o| templates::refkey(o, &refkey.refkey, &articles, &episodes))
        .unwrap())
}
