use super::PooledPg;
use super::{named, redirect, FullEpisode, RenderRucte};
use crate::models::{
    Article, CreatorSet, Episode, IdRefKey, IssueRef, RefKey, RefKeySet,
    Title,
};
use crate::schema::article_refkeys::dsl as ar;
use crate::schema::articles::dsl as a;
use crate::schema::episode_parts::dsl as ep;
use crate::schema::episode_refkeys::dsl as er;
use crate::schema::episodes::dsl as e;
use crate::schema::issues::dsl as i;
use crate::schema::publications::dsl as p;
use crate::schema::refkeys::dsl as r;
use crate::schema::titles::dsl as t;
use crate::templates;
use diesel::dsl::{count_star, min, sql};
use diesel::pg::PgConnection;
use diesel::prelude::*;
use diesel::sql_types::{Integer, SmallInt};
use diesel::QueryDsl;
use failure::Error;
use warp::http::Response;
use warp::{
    self,
    reject::{custom, not_found},
    Rejection, Reply,
};

pub fn get_all_fa(db: &PgConnection) -> Result<Vec<RefKey>, Error> {
    Ok(r::refkeys
        .filter(r::kind.eq(RefKey::FA_ID))
        .order((sql::<Integer>("cast(substr(slug, 1, 2) as int)"), r::slug))
        .load::<IdRefKey>(db)?
        .into_iter()
        .map(|rk| rk.refkey)
        .collect())
}

pub fn refkey_cloud(
    num: i64,
    db: &PgConnection,
) -> Result<Vec<(RefKey, i64, u8)>, Error> {
    let (c_def, c) = named(sql("count(*)"), "c");
    let mut refkeys = r::refkeys
        .left_join(er::episode_refkeys.left_join(e::episodes))
        .select((r::refkeys::all_columns(), c_def))
        .filter(r::kind.eq(RefKey::KEY_ID))
        .group_by(r::refkeys::all_columns())
        .order(c.desc())
        .limit(num)
        .load::<(IdRefKey, i64)>(db)?
        .into_iter()
        .enumerate()
        .map(|(n, (rk, c))| {
            (rk.refkey, c, (8 * (num - n as i64) / num) as u8)
        })
        .collect::<Vec<_>>();
    refkeys.sort_by(|a, b| a.0.name().cmp(&b.0.name()));
    Ok(refkeys)
}

#[allow(clippy::needless_pass_by_value)]
pub fn list_refs(db: PooledPg) -> Result<impl Reply, Rejection> {
    let all = r::refkeys
        .filter(r::kind.eq(RefKey::KEY_ID))
        .left_join(er::episode_refkeys.left_join(e::episodes.left_join(
            ep::episode_parts.left_join(p::publications.left_join(i::issues)),
        )))
        .select((
            r::refkeys::all_columns(),
            sql("count(*)"),
            sql::<SmallInt>(&format!("min({})", IssueRef::MAGIC_Q))
                .nullable(),
            sql::<SmallInt>(&format!("max({})", IssueRef::MAGIC_Q))
                .nullable(),
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

pub fn one_fa(db: PooledPg, slug: String) -> Result<impl Reply, Rejection> {
    one_ref_impl(db, slug, RefKey::FA_ID)
}

pub fn one_ref(db: PooledPg, slug: String) -> Result<impl Reply, Rejection> {
    one_ref_impl(db, slug, RefKey::KEY_ID)
}

#[allow(clippy::needless_pass_by_value)]
fn one_ref_impl(
    db: PooledPg,
    slug: String,
    kind: i16,
) -> Result<impl Reply, Rejection> {
    let refkey = r::refkeys
        .filter(r::kind.eq(&kind))
        .filter(r::slug.eq(&slug))
        .first::<IdRefKey>(&db)
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
            eprintln!("Trying refkey redirect {:?} -> {:?}", slug, target);
            let n = r::refkeys
                .filter(r::kind.eq(&kind))
                .filter(r::slug.eq(&target))
                .select(count_star())
                .first::<i64>(&db)
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

    let articles = a::articles
        .select(a::articles::all_columns())
        .left_join(ar::article_refkeys.left_join(r::refkeys))
        .filter(ar::refkey_id.eq(refkey.id))
        .inner_join(p::publications.inner_join(i::issues))
        .order(min(sql::<SmallInt>("(year-1950)*64 + number")))
        .group_by(a::articles::all_columns())
        .load::<Article>(&db)
        .map_err(custom)?
        .into_iter()
        .map(|article| {
            let refs = RefKeySet::for_article(&article, &db)?;
            let creators = CreatorSet::for_article(&article, &db)?;
            let published = i::issues
                .inner_join(p::publications)
                .select((i::year, (i::number, i::number_str)))
                .filter(p::article_id.eq(article.id))
                .load::<IssueRef>(&db)?;
            Ok((article, refs, creators, published))
        })
        .collect::<Result<Vec<_>, Error>>()
        .map_err(custom)?;

    let episodes = e::episodes
        .left_join(er::episode_refkeys)
        .inner_join(t::titles)
        .filter(er::refkey_id.eq(refkey.id))
        .select((t::titles::all_columns(), e::episodes::all_columns()))
        .inner_join(
            ep::episode_parts
                .inner_join(p::publications.inner_join(i::issues)),
        )
        .order(min(sql::<SmallInt>("(year-1950)*64 + number")))
        .group_by((t::titles::all_columns(), e::episodes::all_columns()))
        .load::<(Title, Episode)>(&db)
        .map_err(custom)?
        .into_iter()
        .map(|(t, ep)| FullEpisode::load_details(ep, &db).map(|e| (t, e)))
        .collect::<Result<Vec<_>, _>>()
        .map_err(custom)?;

    Response::builder()
        .html(|o| templates::refkey(o, &refkey.refkey, &articles, &episodes))
}