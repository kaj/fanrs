use super::{custom, custom_or_404, goh, redirect};
use super::{FullArticle, FullEpisode, OtherContribs, PgFilter, PooledPg};
use crate::models::{
    Article, Creator, CreatorSet, Episode, IssueRef, RefKey, Title,
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
use diesel::dsl::{any, min, sql};
use diesel::prelude::*;
use diesel::sql_types::SmallInt;
use warp::filters::BoxedFilter;
use warp::http::Response;
use warp::{self, Filter, Rejection, Reply};

pub fn routes(s: PgFilter) -> BoxedFilter<(impl Reply,)> {
    use warp::path::{end, param};
    let list = goh().and(end()).and(s.clone()).and_then(list_creators);
    let one = goh().and(s).and(param()).and(end()).and_then(one_creator);
    list.or(one).unify().boxed()
}

#[allow(clippy::needless_pass_by_value)]
fn list_creators(db: PooledPg) -> Result<Response<Vec<u8>>, Rejection> {
    let all = c::creators
        .left_join(
            ca::creator_aliases
                .left_join(
                    eb::episodes_by.left_join(
                        e::episodes.left_join(
                            ep::episode_parts
                                .left_join(p::publications), // .left_join(i::issues)),
                        ),
                    ),
                )
                .left_join(cb::covers_by)
                .left_join(
                    i::issues
                        .on(
                            i::id.eq(p::issue).or(
                            i::id.eq(cb::issue_id))
                        )
                )
        )
        .select((
            c::creators::all_columns(),
            sql("count(distinct episodes.id)"),
            sql("count(distinct covers_by.id)"),
            sql::<SmallInt>(&format!("min({})", IssueRef::MAGIC_Q))
                .nullable(),
            sql::<SmallInt>(&format!("max({})", IssueRef::MAGIC_Q))
                .nullable(),
        ))
        .group_by(c::creators::all_columns())
        .order(c::name)
        .load::<(Creator, i64, i64, Option<i16>, Option<i16>)>(&db)
        .map_err(custom)?
        .into_iter()
        .map(|(creator, n_ep, n_cov, first, last)| {
            (
                creator,
                n_ep,
                n_cov,
                first.map(IssueRef::from_magic),
                last.map(IssueRef::from_magic),
            )
        })
        .collect::<Vec<_>>();
    Response::builder().html(|o| templates::creators(o, &all))
}

#[allow(clippy::needless_pass_by_value)]
fn one_creator(
    db: PooledPg,
    slug: String,
) -> Result<Response<Vec<u8>>, Rejection> {
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
            let published = i::issues
                .inner_join(p::publications)
                .select((i::year, (i::number, i::number_str)))
                .filter(p::article_id.eq(article.id))
                .load::<IssueRef>(&db)?;
            Ok((FullArticle::load(article, &db)?, published))
        })
        .collect::<Result<Vec<_>, diesel::result::Error>>()
        .map_err(custom)?;

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
            let published = i::issues
                .inner_join(p::publications)
                .select((i::year, (i::number, i::number_str)))
                .filter(p::article_id.eq(article.id))
                .load::<IssueRef>(&db)?;
            Ok((FullArticle::load(article, &db)?, published))
        })
        .collect::<Result<Vec<_>, diesel::result::Error>>()
        .map_err(custom)?;

    let covers = CoverSet::by(&creator, &db).map_err(custom)?;
    let others = OtherContribs::for_creator(&creator, &db).map_err(custom)?;

    Response::builder().html(|o| {
        templates::creator(
            o,
            &creator,
            &about,
            &covers,
            &main_episodes,
            &articles_by,
            &others,
        )
    })
}

pub struct CoverSet {
    pub best: Vec<(IssueRef, Option<i16>)>,
    pub all: Vec<(IssueRef, Option<i16>)>,
}
impl CoverSet {
    fn by(
        creator: &Creator,
        db: &PgConnection,
    ) -> Result<CoverSet, diesel::result::Error> {
        let mut covers = i::issues
            .select(((i::year, (i::number, i::number_str)), i::cover_best))
            .inner_join(cb::covers_by.inner_join(ca::creator_aliases))
            .filter(ca::creator_id.eq(creator.id))
            .order((i::cover_best, i::year, i::number))
            .load::<(IssueRef, Option<i16>)>(db)?;

        if covers.len() > 20 {
            let best = covers[0..15].to_vec();
            covers.sort_by(|a, b| a.0.cmp(&b.0));
            Ok(CoverSet { best, all: covers })
        } else {
            covers.sort_by(|a, b| a.0.cmp(&b.0));
            Ok(CoverSet {
                best: covers,
                all: vec![],
            })
        }
    }
    pub fn is_empty(&self) -> bool {
        self.best.is_empty()
    }
    pub fn is_many(&self) -> bool {
        !self.all.is_empty()
    }
    pub fn len(&self) -> usize {
        if self.all.is_empty() {
            self.best.len()
        } else {
            self.all.len()
        }
    }
}
