use super::{FullArticle, FullEpisode, PgPool, Result};
use crate::models::{
    Article, Creator, Episode, IdRefKey, IssueRef, RefKey, Title,
};
use crate::schema::article_refkeys::dsl as ar;
use crate::schema::articles::dsl as a;
use crate::schema::articles_by::dsl as ab;
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
use crate::templates::{RenderRucte, search_html};
use diesel::PgTextExpressionMethods;
use diesel::dsl::{max, sql};
use diesel::prelude::*;
use diesel::sql_types::Text;
use diesel_async::{AsyncPgConnection, RunQueryDsl};
use serde::{Deserialize, Serialize};
use warp::http::Response;
use warp::reply::json;
use warp::{self, Reply};

#[allow(clippy::needless_pass_by_value)]
pub async fn search(
    query: Vec<(String, String)>,
    db: PgPool,
) -> Result<impl Reply> {
    let mut db = db.get().await?;
    let query = SearchQuery::load(query, &mut db).await?;
    let (titles, creators, refkeys, episodes) =
        query.do_search(&mut db).await?;
    Ok(Response::builder().html(|o| {
        search_html(o, &query, &titles, &creators, &refkeys, &episodes)
    })?)
}

pub async fn search_autocomplete(
    query: AcQuery,
    db: PgPool,
) -> Result<impl Reply> {
    let mut db = db.get().await?;
    let q = format!("%{}%", query.q);
    let mut titles = t::titles
        .select((t::title, t::slug))
        .filter(t::title.ilike(q.clone()))
        .order_by(t::title)
        .limit(8)
        .load::<(String, String)>(&mut db)
        .await?
        .into_iter()
        .map(|(t, s)| Completion::title(t, s))
        .collect::<Vec<_>>();
    titles.append(
        &mut ca::creator_aliases
            .inner_join(c::creators)
            .select((sql::<Text>("min(creator_aliases.name)"), c::slug))
            .filter(ca::name.ilike(q.clone()))
            .group_by(c::slug)
            .order(sql::<Text>("min(creator_aliases.name)"))
            .limit(std::cmp::max(2, 8 - titles.len() as i64))
            .load::<(String, String)>(&mut db)
            .await?
            .into_iter()
            .map(|(t, s)| Completion::creator(t, s))
            .collect(),
    );
    titles.append(
        &mut r::refkeys
            .select((r::kind, r::title, r::slug))
            .filter(r::title.ilike(q))
            .filter(r::kind.eq_any([RefKey::FA_ID, RefKey::KEY_ID].as_ref()))
            .order(r::title)
            .limit(std::cmp::max(2, 8 - titles.len() as i64))
            .load::<(i16, String, String)>(&mut db)
            .await?
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
            q: String::new(),
            t: vec![],
            p: vec![],
            k: vec![],
        }
    }
    async fn load(
        query: Vec<(String, String)>,
        db: &mut AsyncPgConnection,
    ) -> Result<Self, diesel::result::Error> {
        let mut result = SearchQuery::empty();
        for (key, val) in query {
            match key.as_ref() {
                "q" => result.q = val,
                "t" => result.t.push(Title::from_slug(val, db).await?),
                "p" => result.p.push(Creator::from_slug(&val, db).await?),
                "k" => result.k.push(IdRefKey::key_from_slug(val, db).await?),
                "f" => result.k.push(IdRefKey::fa_from_slug(val, db).await?),
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
    async fn do_search(
        &self,
        db: &mut AsyncPgConnection,
    ) -> Result<
        (Vec<Title>, Vec<Creator>, Vec<RefKey>, Vec<Hit>),
        diesel::result::Error,
    > {
        let max_hits = 25u8;
        if self.is_empty() {
            return Ok((vec![], vec![], vec![], vec![]));
        }

        let sql_words = self
            .q
            .split_whitespace()
            .map(|word| format!("%{word}%"))
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
            .select((Title::as_select(), Episode::as_select()))
            .into_boxed();

        let mut articles =
            a::articles.select(Article::as_select()).into_boxed();

        for word in &sql_words {
            titles = titles.filter(t::title.ilike(word));
            creators = creators.filter(ca::name.ilike(word));
            refkeys = refkeys.filter(r::title.ilike(word));
            episodes = episodes.filter(
                e::name
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
                ca::id.eq_any(
                    eb::episodes_by
                        .select(eb::creator_alias_id)
                        .inner_join(e::episodes)
                        .filter(e::title_id.eq(title.id)),
                ),
            );
            refkeys = refkeys.filter(
                r::id.eq_any(
                    er::episode_refkeys
                        .select(er::refkey_id)
                        .inner_join(e::episodes)
                        .filter(e::title_id.eq(title.id)),
                ),
            );
            episodes = episodes.filter(e::title_id.eq(title.id));
            articles = articles.filter(
                a::id.eq_any(
                    ar::article_refkeys
                        .select(ar::article_id)
                        .inner_join(r::refkeys)
                        .filter(r::kind.eq(RefKey::TITLE_ID))
                        .filter(r::slug.eq(&title.slug)),
                ),
            );
        }
        for creator in &self.p {
            titles = titles.filter(
                t::id.eq_any(
                    e::episodes
                        .select(e::title_id)
                        .inner_join(
                            eb::episodes_by.inner_join(ca::creator_aliases),
                        )
                        .filter(ca::creator_id.eq(creator.id)),
                ),
            );
            refkeys = refkeys.filter(
                r::id.eq_any(
                    er::episode_refkeys
                        .select(er::refkey_id)
                        .inner_join(e::episodes.inner_join(
                            eb::episodes_by.inner_join(ca::creator_aliases),
                        ))
                        .filter(ca::creator_id.eq(creator.id)),
                ),
            );
            episodes = episodes.filter(
                e::id.eq_any(
                    eb::episodes_by
                        .select(eb::episode_id)
                        .inner_join(ca::creator_aliases)
                        .filter(ca::creator_id.eq(creator.id)),
                ),
            );
            articles = articles.filter({
                let by = ab::articles_by
                    .select(ab::article_id)
                    .inner_join(ca::creator_aliases)
                    .filter(ca::creator_id.eq(creator.id));
                let refs = ar::article_refkeys
                    .select(ar::article_id)
                    .inner_join(r::refkeys)
                    .filter(r::slug.eq(&creator.slug));
                a::id.eq_any(by).or(a::id.eq_any(refs))
            });
        }
        for key in &self.k {
            titles = titles.filter(
                t::id.eq_any(
                    e::episodes
                        .select(e::title_id)
                        .inner_join(er::episode_refkeys)
                        .filter(er::refkey_id.eq(key.id)),
                ),
            );
            creators = creators.filter(
                ca::id.eq_any(
                    eb::episodes_by
                        .select(eb::creator_alias_id)
                        .inner_join(
                            e::episodes.inner_join(er::episode_refkeys),
                        )
                        .filter(er::refkey_id.eq(key.id)),
                ),
            );
            episodes = episodes.filter(
                e::id.eq_any(
                    er::episode_refkeys
                        .select(er::episode_id)
                        .filter(er::refkey_id.eq(key.id)),
                ),
            );
            articles = articles.filter(
                a::id.eq_any(
                    ar::article_refkeys
                        .select(ar::article_id)
                        .filter(ar::refkey_id.eq(key.id)),
                ),
            );
        }

        let creators = if self.q.is_empty() {
            vec![]
        } else {
            creators.limit(max_hits.into()).load(db).await?
        };

        let titles = if self.t.is_empty() && !self.q.is_empty() {
            titles.limit(max_hits.into()).load::<Title>(db).await?
        } else {
            vec![]
        };

        let refkeys = if self.q.is_empty() {
            vec![]
        } else {
            refkeys.limit(max_hits.into()).load(db).await?
        };

        let episodes = episodes
            .order(
                i::issues
                    .left_join(p::publications.left_join(ep::episode_parts))
                    .select(max(i::magic))
                    .filter(ep::episode_id.eq(e::id))
                    .single_value()
                    .desc(),
            )
            .limit(max_hits.into())
            .load::<(Title, Episode)>(db)
            .await?;

        let articles = articles
            .order(
                i::issues
                    .left_join(p::publications)
                    .select(max(i::magic))
                    .filter(a::id.nullable().eq(p::article_id))
                    .single_value()
                    .desc()
                    .nulls_last(),
            )
            .limit(max_hits.into())
            .load(db)
            .await?;

        let mut hits = Vec::with_capacity(episodes.len() + articles.len());
        for (title, ep) in episodes {
            hits.push(Hit::episode(title, ep, db).await?);
        }
        for article in articles {
            hits.push(Hit::article(article, db).await?);
        }
        hits.sort_by(|a, b| b.lastpub().cmp(&a.lastpub()));
        hits.truncate(max_hits.into());

        Ok((titles, creators, refkeys, hits))
    }
}

#[allow(clippy::large_enum_variant)]
pub enum Hit {
    Episode {
        title: Title,
        fe: FullEpisode,
    },
    Article {
        article: FullArticle,
        published: Vec<IssueRef>,
    },
}

impl Hit {
    async fn episode(
        title: Title,
        episode: Episode,
        db: &mut AsyncPgConnection,
    ) -> Result<Hit, diesel::result::Error> {
        FullEpisode::load_details(episode, db)
            .await
            .map(|fe| Hit::Episode { title, fe })
    }

    async fn article(
        article: Article,
        db: &mut AsyncPgConnection,
    ) -> Result<Hit, diesel::result::Error> {
        let published = i::issues
            .inner_join(p::publications)
            .select((i::year, (i::number, i::number_str)))
            .filter(p::article_id.eq(article.id))
            .load::<IssueRef>(db)
            .await?;
        Ok(Hit::Article {
            article: FullArticle::load(article, db).await?,
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
