use super::{
    goh, redirect, wrap, DbError, FullArticle, FullEpisode, OtherContribs,
    PgFilter, PgPool, Result, ViewError,
};
use crate::models::creator_contributions::CreatorContributions;
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
use diesel::dsl::{any, min};
use diesel::prelude::*;
use warp::filters::BoxedFilter;
use warp::http::response::Builder;
use warp::reply::Response;
use warp::{self, Filter, Reply};

pub fn routes(s: PgFilter) -> BoxedFilter<(impl Reply,)> {
    use warp::path::{end, param};
    let list = goh().and(end()).and(s.clone()).then(list_creators);
    let one = goh().and(s).and(param()).and(end()).then(one_creator);
    list.or(one).unify().map(wrap).boxed()
}

async fn list_creators(db: PgPool) -> Result<Response> {
    let db = db.get().await?;
    use crate::models::creator_contributions::creator_contributions::dsl as cc;
    let all = cc::creator_contributions
        .select((
            (cc::id, cc::name, cc::slug),
            cc::score,
            cc::n_episodes,
            cc::n_covers,
            cc::n_articles,
            cc::first_issue,
            cc::latest_issue,
        ))
        .load::<CreatorContributions>(&db)?;
    Ok(Builder::new().html(|o| templates::creators(o, &all))?)
}

async fn one_creator(db: PgPool, slug: String) -> Result<Response> {
    let db = db.get().await?;
    let creator = c::creators
        .filter(c::slug.eq(slug.clone()))
        .first::<Creator>(&db)
        .optional()?;
    let creator = if let Some(creator) = creator {
        creator
    } else {
        let target = slug.replace(['_', '-'], "%").replace(".html", "");
        log::info!("Looking for creator fallback {:?} -> {:?}", slug, target);
        if target == "anderas%eriksson" || target == "andreas%erikssson" {
            return redirect("/who/andreas-eriksson");
        }
        let found = ca::creator_aliases
            .inner_join(c::creators)
            .filter(ca::name.ilike(target.clone()))
            .or_filter(c::slug.ilike(target))
            .select(c::slug)
            .first::<String>(&db)
            .optional()?
            .ok_or(ViewError::NotFound)?;
        log::debug!("Found replacement: {:?}", found);
        return redirect(&format!("/who/{}", found));
    };

    let about_raw = a::articles
        .select(a::articles::all_columns())
        .left_join(ar::article_refkeys.left_join(r::refkeys))
        .filter(r::kind.eq(RefKey::WHO_ID))
        .filter(r::slug.eq(creator.slug.clone()))
        .inner_join(p::publications.inner_join(i::issues))
        .order(min(i::magic))
        .group_by(a::articles::all_columns())
        .load::<Article>(&db)?;
    let mut about = Vec::with_capacity(about_raw.len());
    for article in about_raw.into_iter() {
        let published = i::issues
            .inner_join(p::publications)
            .select((i::year, (i::number, i::number_str)))
            .filter(p::article_id.eq(article.id))
            .load::<IssueRef>(&db)?;
        about.push((FullArticle::load(article, &db)?, published));
    }

    let e_t_columns = (t::titles::all_columns(), e::episodes::all_columns());
    let main_episodes_raw = e::episodes
        .inner_join(eb::episodes_by.inner_join(ca::creator_aliases))
        .inner_join(t::titles)
        .filter(ca::creator_id.eq(creator.id))
        .filter(eb::role.eq(any(CreatorSet::MAIN_ROLES)))
        .select(e_t_columns)
        .inner_join(
            ep::episode_parts
                .inner_join(p::publications.inner_join(i::issues)),
        )
        .order(min(i::magic))
        .group_by(e_t_columns)
        .load::<(Title, Episode)>(&db)?;
    let mut main_episodes = Vec::with_capacity(main_episodes_raw.len());
    for (t, ep) in main_episodes_raw.into_iter() {
        let e = FullEpisode::load_details(ep, &db)?;
        main_episodes.push((t, e));
    }

    let articles_by_raw = a::articles
        .select(a::articles::all_columns())
        .inner_join(ab::articles_by.inner_join(ca::creator_aliases))
        .filter(ca::creator_id.eq(creator.id))
        .inner_join(p::publications.inner_join(i::issues))
        .order(min(i::magic))
        .group_by(a::articles::all_columns())
        .load::<Article>(&db)?;
    let mut articles_by = Vec::with_capacity(articles_by_raw.len());
    for article in articles_by_raw.into_iter() {
        let published = i::issues
            .inner_join(p::publications)
            .select((i::year, (i::number, i::number_str)))
            .filter(p::article_id.eq(article.id))
            .load::<IssueRef>(&db)?;
        articles_by.push((FullArticle::load(article, &db)?, published))
    }

    let covers = CoverSet::by(&creator, &db)?;
    let others = OtherContribs::for_creator(&creator, &db)?;

    Ok(Builder::new().html(|o| {
        templates::creator(
            o,
            &creator,
            &about,
            &covers,
            &main_episodes,
            &articles_by,
            &others,
        )
    })?)
}

pub struct CoverSet {
    pub best: Vec<(IssueRef, Option<i16>)>,
    pub all: Vec<(IssueRef, Option<i16>)>,
}
impl CoverSet {
    fn by(creator: &Creator, db: &PgConnection) -> Result<CoverSet, DbError> {
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
