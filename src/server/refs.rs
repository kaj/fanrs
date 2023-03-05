use super::{
    goh, redirect, wrap, DbError, FullArticle, FullEpisode, PgFilter, PgPool,
    Result, ViewError,
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
use crate::templates::{refkey_html, refkeys_html, RenderRucte};
use diesel::dsl::{count_star, min, sql};
use diesel::prelude::*;
use diesel::sql_types::{Integer, SmallInt};
use diesel::QueryDsl;
use warp::filters::BoxedFilter;
use warp::http::Response;
use warp::{self, Filter};

type ByteResponse = warp::reply::Response;

pub fn fa_route(s: PgFilter) -> BoxedFilter<(ByteResponse,)> {
    use warp::path::{end, param};
    param()
        .and(end())
        .and(goh())
        .and(s)
        .then(one_fa)
        .map(wrap)
        .boxed()
}

pub fn what_routes(s: PgFilter) -> BoxedFilter<(ByteResponse,)> {
    use warp::path::{end, param};
    let list = end().and(goh()).and(s.clone()).then(list_refs);
    let one = param().and(end()).and(goh()).and(s).then(one_ref);
    list.or(one).unify().map(wrap).boxed()
}

pub fn get_all_fa(db: &PgConnection) -> Result<Vec<RefKey>, DbError> {
    Ok(r::refkeys
        .filter(r::kind.eq(RefKey::FA_ID))
        .order((sql::<Integer>("cast(substr(slug, 1, 2) as int)"), r::slug))
        .load::<IdRefKey>(db)?
        .into_iter()
        .map(|rk| rk.refkey)
        .collect())
}

async fn list_refs(db: PgPool) -> Result<ByteResponse> {
    let db = db.get().await?;
    let all = r::refkeys
        .filter(r::kind.eq(RefKey::KEY_ID))
        .left_join(er::episode_refkeys.left_join(e::episodes.left_join(
            ep::episode_parts.left_join(p::publications.left_join(i::issues)),
        )))
        .select((
            r::refkeys::all_columns(),
            sql("count(distinct episodes.id)"),
            sql::<SmallInt>("min(magic)").nullable(),
            sql::<SmallInt>("max(magic)").nullable(),
        ))
        .group_by(r::refkeys::all_columns())
        .order(r::title)
        .load::<(IdRefKey, i64, Option<i16>, Option<i16>)>(&db)?
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
    Ok(Response::builder().html(|o| refkeys_html(o, &all))?)
}

async fn one_fa(slug: String, db: PgPool) -> Result<ByteResponse> {
    one_ref_impl(db, slug, RefKey::FA_ID).await
}

async fn one_ref(slug: String, db: PgPool) -> Result<ByteResponse> {
    one_ref_impl(db, slug, RefKey::KEY_ID).await
}

async fn one_ref_impl(
    db: PgPool,
    slug: String,
    kind: i16,
) -> Result<ByteResponse> {
    let db = db.get().await?;
    let refkey = r::refkeys
        .filter(r::kind.eq(kind))
        .filter(r::slug.eq(slug.clone()))
        .first::<IdRefKey>(&db)
        .optional()?;
    let Some(refkey) = refkey else {
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
            slug.to_lowercase().replace('_', "-").replace(".html", "");
        if target != slug {
            log::debug!("Trying refkey redirect {:?} -> {:?}", slug, target);
            let n = r::refkeys
                .filter(r::kind.eq(kind))
                .filter(r::slug.eq(target.clone()))
                .select(count_star())
                .first::<i64>(&db)?;
            if n == 1 {
                return redirect(&format!(
                    "/{}/{}",
                    if kind == RefKey::FA_ID { "fa" } else { "what" },
                    target,
                ));
            }
        }
        return Err(ViewError::NotFound);
    };

    let raw_articles = a::articles
        .select(a::articles::all_columns())
        .left_join(ar::article_refkeys.left_join(r::refkeys))
        .filter(ar::refkey_id.eq(refkey.id))
        .inner_join(p::publications.inner_join(i::issues))
        .order(min(i::magic))
        .group_by(a::articles::all_columns())
        .load::<Article>(&db)?;

    let mut articles = Vec::with_capacity(raw_articles.len());
    for article in raw_articles {
        let published = i::issues
            .inner_join(p::publications)
            .select((i::year, (i::number, i::number_str)))
            .filter(p::article_id.eq(article.id))
            .load::<IssueRef>(&db)?;
        articles.push((FullArticle::load(article, &db)?, published));
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
        .load::<(Title, Episode)>(&db)?;
    let mut episodes = Vec::with_capacity(raw_episodes.len());
    for (t, ep) in raw_episodes {
        let e = FullEpisode::load_details(ep, &db)?;
        episodes.push((t, e));
    }

    Ok(Response::builder()
        .html(|o| refkey_html(o, &refkey.refkey, &articles, &episodes))?)
}
