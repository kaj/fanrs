use super::{FullEpisode, PooledPg, RenderRucte};
use crate::models::{
    Article, Creator, CreatorSet, Episode, IdRefKey, IssueRef, RefKey,
    RefKeySet, Title,
};
use crate::schema::article_refkeys::dsl as ar;
use crate::schema::articles::dsl as a;
//use crate::schema::articles_by::dsl as ab;
use crate::schema::creator_aliases::dsl as ca;
use crate::schema::creators::dsl as c;
use crate::schema::episode_parts::dsl as ep;
use crate::schema::episode_refkeys::dsl as er;
use crate::schema::episodes::dsl as e;
use crate::schema::episodes_by::dsl as eb;
use crate::schema::issues::dsl as i;
use crate::schema::publications::dsl as p;
use crate::schema::refkeys::dsl as r;
use crate::schema::titles::dsl as t;
use crate::templates;
use diesel::dsl::{any, max, sql};
use diesel::prelude::*;
use diesel::sql_types::{SmallInt, Text};
use diesel::PgTextExpressionMethods;
use failure::Error;
use serde::{Deserialize, Serialize};
use warp::http::Response;
use warp::reply::json;
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

pub fn search_autocomplete(
    db: PooledPg,
    query: AcQuery,
) -> Result<impl Reply, Rejection> {
    let q = format!("%{}%", query.q);
    let mut titles = t::titles
        .select((t::title, t::slug))
        .filter(t::title.ilike(&q))
        .order_by(t::title)
        .limit(8)
        .load::<(String, String)>(&db)
        .map_err(custom)?
        .into_iter()
        .map(|(t, s)| Completion::title(t, s))
        .collect::<Vec<_>>();
    titles.append(
        &mut ca::creator_aliases
            .inner_join(c::creators)
            .select((sql::<Text>("min(creator_aliases.name)"), c::slug))
            .filter(ca::name.ilike(&q))
            .group_by(c::slug)
            .order(sql::<Text>("min(creator_aliases.name)"))
            .limit(std::cmp::max(2, 8 - titles.len() as i64))
            .load::<(String, String)>(&db)
            .map_err(custom)?
            .into_iter()
            .map(|(t, s)| Completion::creator(t, s))
            .collect(),
    );
    titles.append(
        &mut r::refkeys
            .select((r::kind, r::title, r::slug))
            .filter(r::title.ilike(&q))
            .filter(r::kind.eq(any([RefKey::FA_ID, RefKey::KEY_ID].as_ref())))
            .order(r::title)
            .limit(std::cmp::max(2, 8 - titles.len() as i64))
            .load::<(i16, String, String)>(&db)
            .map_err(custom)?
            .into_iter()
            .map(|(k, t, s)| Completion::refkey(k, t, s))
            .collect(),
    );
    titles.sort_by(|a, b| a.t.cmp(&b.t));
    Ok(json(&titles))
}

#[derive(Deserialize)]
pub struct AcQuery {
    pub q: String,
}

#[derive(Serialize)]
pub struct Completion {
    k: &'static str,
    t: String,
    s: String,
}
impl Completion {
    fn title(t: String, s: String) -> Self {
        Completion { k: "t", t, s }
    }
    fn creator(t: String, s: String) -> Self {
        Completion { k: "p", t, s }
    }
    fn refkey(k: i16, t: String, s: String) -> Self {
        Completion {
            k: if k == RefKey::FA_ID { "f" } else { "k" },
            t,
            s,
        }
    }
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
    ) -> Result<(Vec<Title>, Vec<Creator>, Vec<RefKey>, Vec<Hit>), Error>
    {
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

        let mut articles = a::articles
            .select(a::articles::all_columns())
            .inner_join(p::publications.inner_join(i::issues))
            .into_boxed();

        for word in &sql_words {
            titles = titles.filter(t::title.ilike(word));
            creators = creators.filter(ca::name.ilike(word));
            refkeys = refkeys.filter(r::title.ilike(word));
            episodes = episodes.filter(
                e::episode
                    .ilike(word)
                    .or(e::orig_episode.ilike(word))
                    .or(e::teaser.ilike(word))
                    .or(e::note.ilike(word))
                    .or(e::copyright.ilike(word)),
            );
            articles = articles.filter(
                a::title
                    .ilike(word)
                    .or(a::subtitle.ilike(word))
                    .or(a::note.ilike(word)),
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
            articles = articles.filter(
                a::id.eq(any(ar::article_refkeys
                    .select(ar::article_id)
                    .inner_join(r::refkeys)
                    .filter(r::kind.eq(RefKey::TITLE_ID))
                    .filter(r::slug.eq(&title.slug)))),
            );
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
            articles = articles.filter(a::id.eq(any(
                // Can this be done as a union in diesel?
                sql(&format!(
                    "select article_id from articles_by \
                     inner join creator_aliases ca on by_id=ca.id \
                     where ca.creator_id={} \
                     union \
                     select article_id from article_refkeys ar \
                     inner join refkeys r on r.id = ar.refkey_id \
                     where slug='{}'",
                    creator.id, creator.slug
                )),
                // ab::articles_by
                //     .select(ab::article_id)
                //     .inner_join(ca::creator_aliases)
                //     .filter(ca::creator_id.eq(creator.id)),
                // **union**
                // ar::article_refkeys
                //     .select(ar::article_id)
                //     .inner_join(r::refkeys)
                //     .filter(r::slug.eq(&creator.slug)),
            )));
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
            articles = articles.filter(
                a::id.eq(any(ar::article_refkeys
                    .select(ar::article_id)
                    .filter(ar::refkey_id.eq(key.id)))),
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

        let mut episodes = episodes
            .order(max(sql::<SmallInt>("(year-1950)*64 + number")).desc())
            .group_by((t::titles::all_columns(), e::episodes::all_columns()))
            .limit(max_hits)
            .load::<(Title, Episode)>(db)?
            .into_iter()
            .map(|(title, ep)| Hit::episode(title, ep, db))
            .collect::<Result<Vec<_>, _>>()?;

        let articles = articles
            .order(max(sql::<SmallInt>("(year-1950)*64 + number")).desc())
            .group_by(a::articles::all_columns())
            .limit(max_hits)
            .load(db)?
            .into_iter()
            .map(|article| Hit::article(article, db))
            .collect::<Result<Vec<_>, Error>>()?;

        episodes.extend(articles.into_iter());
        episodes.sort_by(|a, b| b.lastpub().cmp(&a.lastpub()));
        episodes.truncate(max_hits as usize);

        Ok((titles, creators, refkeys, episodes))
    }
}

#[allow(clippy::large_enum_variant)]
pub enum Hit {
    Episode {
        title: Title,
        fe: FullEpisode,
    },
    Article {
        article: Article,
        refs: RefKeySet,
        creators: CreatorSet,
        published: Vec<IssueRef>,
    },
}

impl Hit {
    fn episode(
        title: Title,
        episode: Episode,
        db: &PgConnection,
    ) -> Result<Hit, Error> {
        FullEpisode::load_details(episode, db)
            .map(|fe| Hit::Episode { title, fe })
    }

    fn article(article: Article, db: &PgConnection) -> Result<Hit, Error> {
        let refs = RefKeySet::for_article(&article, db)?;
        let creators = CreatorSet::for_article(&article, db)?;
        let published = i::issues
            .inner_join(p::publications)
            .select((i::year, (i::number, i::number_str)))
            .filter(p::article_id.eq(article.id))
            .load::<IssueRef>(db)?;
        Ok(Hit::Article {
            article,
            refs,
            creators,
            published,
        })
    }

    fn lastpub(&self) -> Option<&IssueRef> {
        match self {
            Hit::Episode { fe, .. } => fe.published.last(),
            Hit::Article { published, .. } => published.last(),
        }
    }
}
