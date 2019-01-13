use super::{PooledPg, RenderRucte};
use crate::models::{
    Creator, CreatorSet, Episode, IdRefKey, PartInIssue, RefKey, RefKeySet,
    Title,
};
// Article, Issue, IssueRef, Part
use crate::templates;
use diesel::prelude::*;
use diesel::QueryDsl;
use failure::Error;
use warp::http::Response;
use warp::{self, reject::custom, Rejection, Reply};

#[allow(clippy::needless_pass_by_value)]
pub fn search(
    db: PooledPg,
    query: Vec<(String, String)>,
) -> Result<impl Reply, Rejection> {
    let query = SearchQuery::load(query, &db).map_err(custom)?;
    let (titles, creators, refkeys, episodes) =
        query.do_search(&db).map_err(custom)?;
    Response::builder().html(|o| {
        templates::search(o, &query, &titles, &creators, &refkeys, &episodes)
    })
}

#[derive(Debug)]
pub struct SearchQuery {
    pub q: String,
    pub t: Vec<Title>,
    pub p: Vec<Creator>,
    pub k: Vec<IdRefKey>,
}

impl SearchQuery {
    pub fn empty() -> Self {
        SearchQuery {
            q: "".into(),
            t: vec![],
            p: vec![],
            k: vec![],
        }
    }
    fn load(
        query: Vec<(String, String)>,
        db: &PooledPg,
    ) -> Result<Self, Error> {
        let mut result = SearchQuery::empty();
        for (key, val) in query {
            match key.as_ref() {
                "q" => result.q = val,
                "t" => result.t.push(Title::from_slug(&val, db)?),
                "p" => result.p.push(Creator::from_slug(&val, db)?),
                "k" => result.k.push(IdRefKey::key_from_slug(&val, db)?),
                "f" => result.k.push(IdRefKey::fa_from_slug(&val, db)?),
                _ => (), // ignore unknown query parameters
            }
        }
        Ok(result)
    }
    fn is_empty(&self) -> bool {
        self.q.is_empty()
            && self.t.is_empty()
            && self.p.is_empty()
            && self.k.is_empty()
    }
    fn do_search(
        &self,
        db: &PooledPg,
    ) -> Result<
        (
            Vec<Title>,
            Vec<Creator>,
            Vec<RefKey>,
            Vec<(Title, Episode, RefKeySet, CreatorSet, Vec<PartInIssue>)>,
        ),
        Error,
    > {
        use crate::schema::creator_aliases::dsl as ca;
        use crate::schema::creators::dsl as c;
        //use crate::schema::article_refkeys::dsl as ar;
        //use crate::schema::articles::dsl as a;
        use crate::schema::episode_parts::dsl as ep;
        use crate::schema::episode_refkeys::dsl as er;
        use crate::schema::episodes::dsl as e;
        use crate::schema::episodes_by::dsl as eb;
        use crate::schema::issues::dsl as i;
        use crate::schema::publications::dsl as p;
        use crate::schema::refkeys::dsl as r;
        use crate::schema::titles::dsl as t;
        use diesel::dsl::{any, max, sql};
        use diesel::sql_types::SmallInt;
        use diesel::PgTextExpressionMethods;

        let max_hits = 25;
        if self.is_empty() {
            return Ok((vec![], vec![], vec![], vec![]));
        }

        let sql_words = self
            .q
            .split_whitespace()
            .map(|word| format!("%{}%", word))
            .collect::<Vec<_>>();

        let mut creators = c::creators
            .select(c::creators::all_columns())
            .left_join(ca::creator_aliases)
            .into_boxed();

        let mut titles =
            t::titles.select(t::titles::all_columns()).into_boxed();

        let mut refkeys = r::refkeys
            .select((r::kind, r::title, r::slug))
            .filter(r::kind.eq(RefKey::FA_ID).or(r::kind.eq(RefKey::KEY_ID)))
            .into_boxed();

        let mut episodes = e::episodes
            .inner_join(t::titles)
            .select((t::titles::all_columns(), e::episodes::all_columns()))
            .inner_join(
                ep::episode_parts
                    .inner_join(p::publications.inner_join(i::issues)),
            )
            .into_boxed();

        for word in &sql_words {
            titles = titles.filter(t::title.ilike(word));
            creators = creators.filter(ca::name.ilike(word));
            // TODO Every `like` here should be `ilike` when supported!
            refkeys = refkeys.filter(r::title.like(word));
            episodes = episodes.filter(
                e::episode
                    .like(word)
                    .or(e::teaser.like(word))
                    .or(e::note.like(word))
                    .or(e::copyright.like(word)),
            );
        }

        for title in &self.t {
            creators = creators.filter(
                ca::id.eq(any(eb::episodes_by
                    .select(eb::by_id)
                    .inner_join(e::episodes)
                    .filter(e::title.eq(title.id)))),
            );
            refkeys = refkeys.filter(
                r::id.eq(any(er::episode_refkeys
                    .select(er::refkey_id)
                    .inner_join(e::episodes)
                    .filter(e::title.eq(title.id)))),
            );
            episodes = episodes.filter(e::title.eq(title.id));
        }
        for creator in &self.p {
            titles = titles.filter(
                t::id.eq(any(e::episodes
                    .select(e::title)
                    .inner_join(
                        eb::episodes_by.inner_join(ca::creator_aliases),
                    )
                    .filter(ca::creator_id.eq(creator.id)))),
            );
            refkeys = refkeys.filter(
                r::id.eq(any(er::episode_refkeys
                    .select(er::refkey_id)
                    .inner_join(e::episodes.inner_join(
                        eb::episodes_by.inner_join(ca::creator_aliases),
                    ))
                    .filter(ca::creator_id.eq(creator.id)))),
            );
            episodes = episodes.filter(
                e::id.eq(any(eb::episodes_by
                    .select(eb::episode_id)
                    .inner_join(ca::creator_aliases)
                    .filter(ca::creator_id.eq(creator.id)))),
            );
        }
        for key in &self.k {
            titles = titles.filter(
                t::id.eq(any(e::episodes
                    .select(e::title)
                    .inner_join(er::episode_refkeys)
                    .filter(er::refkey_id.eq(key.id)))),
            );
            creators = creators.filter(
                ca::id.eq(any(eb::episodes_by
                    .select(eb::by_id)
                    .inner_join(e::episodes.inner_join(er::episode_refkeys))
                    .filter(er::refkey_id.eq(key.id)))),
            );
            episodes = episodes.filter(
                e::id.eq(any(er::episode_refkeys
                    .select(er::episode_id)
                    .filter(er::refkey_id.eq(key.id)))),
            );
        }

        let creators = if self.q.is_empty() {
            vec![]
        } else {
            creators
                .group_by(c::creators::all_columns())
                .limit(max_hits)
                .load(db)?
        };

        let titles = if self.t.is_empty() && !self.q.is_empty() {
            titles.limit(max_hits).load::<Title>(db)?
        } else {
            vec![]
        };

        let refkeys = if self.q.is_empty() {
            vec![]
        } else {
            refkeys.limit(max_hits).load(db)?
        };

        let episodes = episodes
            .order(max(sql::<SmallInt>("(year-1950)*64 + number")).desc())
            .group_by((t::titles::all_columns(), e::episodes::all_columns()))
            .limit(max_hits)
            .load::<(Title, Episode)>(db)?
            .into_iter()
            .map(|(title, episode)| {
                let refs = RefKeySet::for_episode(&episode, db).unwrap();
                let creators = CreatorSet::for_episode(&episode, db).unwrap();
                let published = i::issues
                    .inner_join(p::publications.inner_join(ep::episode_parts))
                    .select((
                        (i::year, (i::number, i::number_str)),
                        (ep::id, ep::part_no, ep::part_name),
                    ))
                    .filter(ep::episode.eq(episode.id))
                    .load::<PartInIssue>(db)
                    .unwrap();
                (title, episode, refs, creators, published)
            })
            .collect::<Vec<_>>();
        Ok((titles, creators, refkeys, episodes))
    }
}
