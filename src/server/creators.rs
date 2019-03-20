use super::{custom, custom_or_404, named, redirect, sortable_issue};
use super::{FullEpisode, PartsPublished, PooledPg};
use crate::models::{
    Article, Creator, CreatorSet, Episode, IssueRef, RefKey, RefKeySet, Title,
};
use crate::schema::article_refkeys::dsl as ar;
use crate::schema::articles::dsl as a;
use crate::schema::articles_by::dsl as ab;
use crate::schema::covers_by::dsl as cb;
use crate::schema::creator_aliases::dsl as ca;
use crate::schema::creators::dsl as c;
use crate::schema::episode_parts::dsl as ep;
use crate::schema::episodes::dsl as e;
use crate::schema::episodes_by::dsl as eb;
use crate::schema::issues::dsl as i;
use crate::schema::publications::dsl as p;
use crate::schema::refkeys::dsl as r;
use crate::schema::titles::dsl as t;
use crate::templates::{self, RenderRucte};
use diesel::dsl::{all, any, min, sql};
use diesel::prelude::*;
use diesel::result::Error;
use diesel::sql_types::{BigInt, SmallInt};
use std::collections::BTreeMap;
use warp::http::Response;
use warp::{self, Rejection, Reply};

pub fn creator_cloud(
    num: i64,
    db: &PgConnection,
) -> Result<Vec<(Creator, i64, u8)>, Error> {
    let (c_ep, c_ep_n) =
        named(sql::<BigInt>("count(distinct episode_id)"), "n");
    let mut creators = c::creators
        .left_join(ca::creator_aliases.left_join(eb::episodes_by))
        .filter(eb::role.eq(any(CreatorSet::MAIN_ROLES)))
        .select((c::creators::all_columns(), c_ep))
        .group_by(c::creators::all_columns())
        .order(c_ep_n.desc())
        .limit(num)
        .load::<(Creator, i64)>(db)?
        .into_iter()
        .enumerate()
        .map(|(n, (creator, c))| {
            (creator, c, (8 * (num - n as i64) / num) as u8)
        })
        .collect::<Vec<_>>();
    creators.sort_by(|a, b| a.0.name.cmp(&b.0.name));
    Ok(creators)
}

#[allow(clippy::needless_pass_by_value)]
pub fn list_creators(db: PooledPg) -> Result<impl Reply, Rejection> {
    let all = c::creators
        .left_join(
            ca::creator_aliases.left_join(
                eb::episodes_by.left_join(
                    e::episodes.left_join(
                        ep::episode_parts
                            .left_join(p::publications.left_join(i::issues)),
                    ),
                ),
            ),
        )
        .select((
            c::creators::all_columns(),
            sql("count(distinct episodes.id)"),
            sql::<SmallInt>(&format!("min({})", IssueRef::MAGIC_Q))
                .nullable(),
            sql::<SmallInt>(&format!("max({})", IssueRef::MAGIC_Q))
                .nullable(),
        ))
        .group_by(c::creators::all_columns())
        .order(c::name)
        .load::<(Creator, i64, Option<i16>, Option<i16>)>(&db)
        .map_err(custom)?
        .into_iter()
        .map(|(creator, c, first, last)| {
            (
                creator,
                c,
                first.map(IssueRef::from_magic),
                last.map(IssueRef::from_magic),
            )
        })
        .collect::<Vec<_>>();
    Response::builder().html(|o| templates::creators(o, &all))
}

#[allow(clippy::needless_pass_by_value)]
pub fn one_creator(
    db: PooledPg,
    slug: String,
) -> Result<impl Reply, Rejection> {
    let creator = c::creators
        .filter(c::slug.eq(&slug))
        .first::<Creator>(&db)
        .optional()
        .map_err(custom)?;
    let creator = if let Some(creator) = creator {
        creator
    } else {
        let target = slug
            .replace('_', "%")
            .replace('-', "%")
            .replace(".html", "");
        eprintln!("Looking for creator fallback {:?} -> {:?}", slug, target);
        if target == "anderas%eriksson" || target == "andreas%erikssson" {
            return redirect("/who/andreas_eriksson");
        }
        let found = ca::creator_aliases
            .inner_join(c::creators)
            .filter(ca::name.ilike(&target))
            .or_filter(c::slug.ilike(&target))
            .select(c::slug)
            .first::<String>(&db)
            .map_err(custom_or_404)?;
        eprintln!("Found replacement: {:?}", found);
        return redirect(&format!("/who/{}", found));
    };

    let about = a::articles
        .select(a::articles::all_columns())
        .left_join(ar::article_refkeys.left_join(r::refkeys))
        .filter(r::kind.eq(RefKey::WHO_ID))
        .filter(r::slug.eq(&creator.slug))
        .inner_join(p::publications.inner_join(i::issues))
        .order(min(sql::<SmallInt>("(year-1950)*64 + number")))
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

    let mut covers = i::issues
        .select(((i::year, (i::number, i::number_str)), i::cover_best))
        .inner_join(cb::covers_by.inner_join(ca::creator_aliases))
        .filter(ca::creator_id.eq(creator.id))
        .order((i::cover_best, i::year, i::number))
        .load::<(IssueRef, Option<i16>)>(&db)
        .map_err(custom)?;

    let (covers, all_covers) = if covers.len() > 20 {
        let best = covers[0..15].to_vec();
        covers.sort_by(|a, b| a.0.cmp(&b.0));
        (best, covers)
    } else {
        covers.sort_by(|a, b| a.0.cmp(&b.0));
        (covers, vec![])
    };

    let e_t_columns = (t::titles::all_columns(), e::episodes::all_columns());
    let main_episodes = e::episodes
        .inner_join(eb::episodes_by.inner_join(ca::creator_aliases))
        .inner_join(t::titles)
        .filter(ca::creator_id.eq(creator.id))
        .filter(eb::role.eq(any(CreatorSet::MAIN_ROLES)))
        .select(e_t_columns)
        .inner_join(
            ep::episode_parts
                .inner_join(p::publications.inner_join(i::issues)),
        )
        .order(min(sql::<SmallInt>("(year-1950)*64 + number")))
        .group_by(e_t_columns)
        .load::<(Title, Episode)>(&db)
        .map_err(custom)?
        .into_iter()
        .map(|(t, ep)| FullEpisode::load_details(ep, &db).map(|e| (t, e)))
        .collect::<Result<Vec<_>, _>>()
        .map_err(custom)?;

    let articles_by = a::articles
        .select(a::articles::all_columns())
        .inner_join(ab::articles_by.inner_join(ca::creator_aliases))
        .filter(ca::creator_id.eq(creator.id))
        .inner_join(p::publications.inner_join(i::issues))
        .order(min(sql::<SmallInt>("(year-1950)*64 + number")))
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

    let oe_columns = (t::titles::all_columns(), e::id, e::episode);
    let other_episodes = e::episodes
        .inner_join(eb::episodes_by.inner_join(ca::creator_aliases))
        .inner_join(t::titles)
        .filter(ca::creator_id.eq(creator.id))
        .filter(eb::role.ne(all(CreatorSet::MAIN_ROLES)))
        .select(oe_columns)
        .inner_join(
            ep::episode_parts
                .inner_join(p::publications.inner_join(i::issues)),
        )
        .order(min(sortable_issue()))
        .group_by(oe_columns)
        .load::<(Title, i32, Option<String>)>(&db)
        .map_err(custom)?;

    let mut oe: BTreeMap<_, Vec<_>> = BTreeMap::new();
    for (title, episode_id, episode) in other_episodes {
        let published =
            PartsPublished::for_episode_id(episode_id, &db).unwrap();
        oe.entry(title).or_default().push((episode, published));
    }

    let o_roles = eb::episodes_by
        .inner_join(ca::creator_aliases)
        .filter(ca::creator_id.eq(creator.id))
        .filter(eb::role.ne(all(CreatorSet::MAIN_ROLES)))
        .select(eb::role)
        .distinct()
        .load::<String>(&db)
        .map_err(custom)?
        .into_iter()
        .map(|r| match r.as_ref() {
            "color" => "färgläggare",
            "redax" => "redaktion",
            "xlat" => "översättare",
            "textning" => "textsättare",
            _ => "något annat",
        })
        .collect::<Vec<_>>()
        .join(", ");

    Response::builder().html(|o| {
        templates::creator(
            o,
            &creator,
            &about,
            &covers,
            &all_covers,
            &main_episodes,
            &articles_by,
            &o_roles,
            &oe,
        )
    })
}
