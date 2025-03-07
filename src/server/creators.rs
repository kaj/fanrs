use super::{
    DbError, FullArticle, FullEpisode, OtherContribs, PgFilter, PgPool,
    Result, ViewError, goh, redirect, wrap,
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
use crate::templates::{RenderRucte, creator_html, creators_html};
use diesel::dsl::min;
use diesel::prelude::*;
use diesel_async::{AsyncPgConnection, RunQueryDsl};
use tracing::{debug, info, instrument};
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
    use crate::models::creator_contributions::creator_contributions::dsl as cc;
    let mut db = db.get().await?;
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
        .load::<CreatorContributions>(&mut db)
        .await?;
    Ok(Builder::new().html(|o| creators_html(o, &all))?)
}

#[instrument(skip(db), err)]
async fn one_creator(db: PgPool, slug: String) -> Result<Response> {
    let mut db = db.get().await?;
    let creator = c::creators
        .filter(c::slug.eq(slug.clone()))
        .first::<Creator>(&mut db)
        .await
        .optional()?;
    let Some(creator) = creator else {
        let target = slug.replace(['_', '-'], "%").replace(".html", "");
        info!("Looking for creator fallback {:?} -> {:?}", slug, target);
        if target == "anderas%eriksson" || target == "andreas%erikssson" {
            return redirect("/who/andreas-eriksson");
        }
        let found = ca::creator_aliases
            .inner_join(c::creators)
            .filter(ca::name.ilike(target.clone()))
            .or_filter(c::slug.ilike(target))
            .select(c::slug)
            .first::<String>(&mut db)
            .await
            .optional()?
            .ok_or(ViewError::NotFound)?;
        debug!("Found replacement: {found:?}");
        return redirect(&format!("/who/{found}"));
    };

    let about_raw = a::articles
        .select(a::articles::all_columns())
        .left_join(ar::article_refkeys.left_join(r::refkeys))
        .filter(r::kind.eq(RefKey::WHO_ID))
        .filter(r::slug.eq(creator.slug.clone()))
        .inner_join(p::publications.inner_join(i::issues))
        .order(min(i::magic))
        .group_by(a::articles::all_columns())
        .load::<Article>(&mut db)
        .await?;
    let mut about = Vec::with_capacity(about_raw.len());
    for article in about_raw {
        let published = i::issues
            .inner_join(p::publications)
            .select((i::year, (i::number, i::number_str)))
            .filter(p::article_id.eq(article.id))
            .load::<IssueRef>(&mut db)
            .await?;
        about.push((FullArticle::load(article, &mut db).await?, published));
    }

    let main_episodes_raw = e::episodes
        .inner_join(t::titles)
        .select((Episode::as_select(), Title::as_select()))
        .filter(
            e::id.eq_any(
                eb::episodes_by
                    .select(eb::episode_id)
                    .filter(
                        eb::creator_alias_id.eq_any(
                            ca::creator_aliases
                                .select(ca::id)
                                .filter(ca::creator_id.eq(creator.id)),
                        ),
                    )
                    .filter(eb::role.eq_any(CreatorSet::MAIN_ROLES)),
            ),
        )
        .order(
            i::issues
                .left_join(p::publications.left_join(ep::episode_parts))
                .select(min(i::magic))
                .filter(ep::episode_id.eq(e::id))
                .single_value(),
        )
        .load::<(Episode, Title)>(&mut db)
        .await?;
    let mut main_episodes = Vec::with_capacity(main_episodes_raw.len());
    for (ep, t) in main_episodes_raw {
        let e = FullEpisode::load_details(ep, &mut db).await?;
        main_episodes.push((t, e));
    }

    let articles_by_raw = a::articles
        .select(a::articles::all_columns())
        .inner_join(ab::articles_by.inner_join(ca::creator_aliases))
        .filter(ca::creator_id.eq(creator.id))
        .inner_join(p::publications.inner_join(i::issues))
        .order(min(i::magic))
        .group_by(a::articles::all_columns())
        .load::<Article>(&mut db)
        .await?;
    let mut articles_by = Vec::with_capacity(articles_by_raw.len());
    for article in articles_by_raw {
        let published = i::issues
            .inner_join(p::publications)
            .select((i::year, (i::number, i::number_str)))
            .filter(p::article_id.eq(article.id))
            .load::<IssueRef>(&mut db)
            .await?;
        articles_by
            .push((FullArticle::load(article, &mut db).await?, published));
    }

    let covers = CoverSet::by(&creator, &mut db).await?;
    let others = OtherContribs::for_creator(&creator, &mut db).await?;

    Ok(Builder::new().html(|o| {
        creator_html(
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
    async fn by(
        creator: &Creator,
        db: &mut AsyncPgConnection,
    ) -> Result<CoverSet, DbError> {
        let mut covers = i::issues
            .select(((i::year, (i::number, i::number_str)), i::cover_best))
            .inner_join(cb::covers_by.inner_join(ca::creator_aliases))
            .filter(ca::creator_id.eq(creator.id))
            .order((i::cover_best, i::year, i::number))
            .load::<(IssueRef, Option<i16>)>(db)
            .await?;

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
