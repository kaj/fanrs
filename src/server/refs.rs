use super::{
    custom, goh, redirect, FullArticle, FullEpisode, PgFilter, PgPool,
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
use failure::Error;
use tokio_diesel::{AsyncRunQueryDsl, OptionalExtension};
use warp::filters::BoxedFilter;
use warp::http::Response;
use warp::reject::not_found;
use warp::{self, Filter, Rejection};

type ByteResponse = warp::reply::Response;

pub fn what_routes(s: PgFilter) -> BoxedFilter<(ByteResponse,)> {
    use warp::path::{end, param};
    let list = goh().and(end()).and(s.clone()).and_then(list_refs);
    let one = goh().and(s).and(param()).and(end()).and_then(one_ref);
    list.or(one).unify().boxed()
}

pub async fn get_all_fa(db: &PgPool) -> Result<Vec<RefKey>, Error> {
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
async fn list_refs(db: PgPool) -> Result<ByteResponse, Rejection> {
    let db = db.get().map_err(custom)?;
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
        .load::<(IdRefKey, i64, Option<i16>, Option<i16>)>(&db)
        .map_err(custom)?
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
    Response::builder().html(|o| templates::refkeys(o, &all))
}

pub async fn one_fa(
    db: PgPool,
    slug: String,
) -> Result<ByteResponse, Rejection> {
    one_ref_impl(db, slug, RefKey::FA_ID).await
}

async fn one_ref(
    db: PgPool,
    slug: String,
) -> Result<ByteResponse, Rejection> {
    one_ref_impl(db, slug, RefKey::KEY_ID).await
}

async fn one_ref_impl(
    db: PgPool,
    slug: String,
    kind: i16,
) -> Result<ByteResponse, Rejection> {
    let refkey = r::refkeys
        .filter(r::kind.eq(kind))
        .filter(r::slug.eq(slug.clone()))
        .first_async::<IdRefKey>(&db)
        .await
        .optional()
        .map_err(custom)?;
    let refkey = if let Some(refkey) = refkey {
        refkey
    } else {
        if kind == RefKey::FA_ID {
            // Some special cases
            if slug == "17.1" {
                return redirect("/fa/17j");
            } else if slug == "22.1" {
                return redirect("/fa/22k");
            } else if slug == "22.2" {
                return redirect("/fa/22j");
            }
        }
        if kind == RefKey::KEY_ID {
            if slug == "christophe_derrant" {
                return redirect("/what/christophe-d-errant");
            } else if slug == "olangofolket" {
                return redirect("/what/olango-folket");
            } else if slug == "/what/piratpete" {
                return redirect("/what/pirat-pete");
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
                .await
                .map_err(custom)?;
            if n == 1 {
                return redirect(&format!(
                    "/{}/{}",
                    if kind == RefKey::FA_ID { "fa" } else { "what" },
                    target,
                ));
            }
        }
        return Err(not_found());
    };

    let raw_articles = a::articles
        .select(a::articles::all_columns())
        .left_join(ar::article_refkeys.left_join(r::refkeys))
        .filter(ar::refkey_id.eq(refkey.id))
        .inner_join(p::publications.inner_join(i::issues))
        .order(min(i::magic))
        .group_by(a::articles::all_columns())
        .load_async::<Article>(&db)
        .await
        .map_err(custom)?;

    let mut articles = Vec::with_capacity(raw_articles.len());
    for article in raw_articles.into_iter() {
        let published = i::issues
            .inner_join(p::publications)
            .select((i::year, (i::number, i::number_str)))
            .filter(p::article_id.eq(article.id))
            .load_async::<IssueRef>(&db)
            .await
            .map_err(custom)?;
        articles.push((
            FullArticle::load_async(article, &db)
                .await
                .map_err(custom)?,
            published,
        ));
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
        .await
        .map_err(custom)?;
    let mut episodes = Vec::with_capacity(raw_episodes.len());
    for (t, ep) in raw_episodes.into_iter() {
        let e = FullEpisode::load_details_async(ep, &db)
            .await
            .map_err(custom)?;
        episodes.push((t, e));
    }

    Response::builder()
        .html(|o| templates::refkey(o, &refkey.refkey, &articles, &episodes))
}
