mod publist;
mod render_ructe;
pub mod search;

pub use self::publist::PartsPublished;
use self::render_ructe::RenderRucte;
use self::search::{search, search_autocomplete};
use crate::models::{
    Article, Creator, CreatorSet, Episode, IdRefKey, Issue, IssueRef, Part,
    RefKey, RefKeySet, Title,
};
use crate::schema::article_refkeys::dsl as ar;
use crate::schema::articles::dsl as a;
use crate::schema::articles_by::dsl as ab;
use crate::schema::covers_by::dsl as cb;
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
use chrono::{Duration, Utc};
use diesel::dsl::{all, any, count_star, min, not, sql};
use diesel::pg::PgConnection;
use diesel::prelude::*;
use diesel::r2d2::{ConnectionManager, Pool, PooledConnection};
use diesel::sql_types::{BigInt, Integer};
use diesel::QueryDsl;
use failure::Error;
use mime::IMAGE_JPEG;
use std::collections::BTreeMap;
use std::str::FromStr;
use warp::http::status::StatusCode;
use warp::http::Response;
use warp::path::Tail;
use warp::{
    self,
    http::header::{CONTENT_TYPE, EXPIRES},
    reject::{custom, not_found},
    Filter, Rejection, Reply,
};

type PooledPg = PooledConnection<ConnectionManager<PgConnection>>;
type PgPool = Pool<ConnectionManager<PgConnection>>;

pub fn run(db_url: &str) -> Result<(), Error> {
    let pool = pg_pool(db_url);
    let s = warp::any()
        .and_then(move || match pool.get() {
            Ok(conn) => Ok(conn),
            Err(e) => {
                eprintln!("Failed to get a db connection: {}", e);
                Err(custom(e))
            }
        })
        .boxed();
    let s = move || s.clone();
    use warp::filters::query::query;
    use warp::{get2 as get, path, path::end};
    let routes = warp::any()
        .and(get().and(path("s")).and(path::tail()).and_then(static_file))
        .or(get()
            .and(path("c"))
            .and(s())
            .and(path::param())
            .and(end())
            .and_then(cover_image))
        .or(get().and(end()).and(s()).and_then(frontpage))
        .or(get()
            .and(path("search"))
            .and(end())
            .and(s())
            .and(query())
            .and_then(search))
        .or(get()
            .and(path("ac"))
            .and(end())
            .and(s())
            .and(query())
            .and_then(search_autocomplete))
        .or(get()
            .and(path("titles"))
            .and(end())
            .and(s())
            .and_then(list_titles))
        .or(get()
            .and(path("titles"))
            .and(s())
            .and(path::param())
            .and(end())
            .and_then(one_title))
        .or(get()
            .and(path("fa"))
            .and(s())
            .and(path::param())
            .and(end())
            .and_then(one_fa))
        .or(get()
            .and(path("what"))
            .and(s())
            .and(path::param())
            .and(end())
            .and_then(one_ref))
        .or(get()
            .and(path("what"))
            .and(end())
            .and(s())
            .and_then(list_refs))
        .or(get()
            .and(path("who"))
            .and(s())
            .and(path::param())
            .and(end())
            .and_then(one_creator))
        .or(get()
            .and(path("who"))
            .and(end())
            .and(s())
            .and_then(list_creators))
        .or(get()
            .and(s())
            .and(path::param())
            .and(end())
            .and_then(list_year))
        .or(get()
            .and(s())
            .and(path::param())
            .and(end())
            .and_then(oldslug_title))
        .recover(customize_error);
    warp::serve(routes).run(([127, 0, 0, 1], 1536));
    Ok(())
}

/// Handler for static files.
/// Create a response from the file data with a correct content type
/// and a far expires header (or a 404 if the file does not exist).
#[allow(clippy::needless_pass_by_value)]
fn static_file(name: Tail) -> Result<impl Reply, Rejection> {
    use crate::templates::statics::StaticFile;
    if let Some(data) = StaticFile::get(name.as_str()) {
        let far_expires = Utc::now() + Duration::days(180);
        Ok(Response::builder()
            .header(CONTENT_TYPE, data.mime.as_ref())
            .header(EXPIRES, far_expires.to_rfc2822())
            .body(data.content))
    } else {
        println!("Static file {:?} not found", name);
        Err(not_found())
    }
}

fn pg_pool(database_url: &str) -> PgPool {
    let manager = ConnectionManager::<PgConnection>::new(database_url);
    Pool::new(manager).expect("Postgres connection pool could not be created")
}

struct CoverRef {
    year: i16,
    number: i16,
}

impl FromStr for CoverRef {
    type Err = u8;
    /// expect fYYYY-NN.jpg
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if !s.starts_with('f') {
            return Err(0);
        }
        let p1 = s.find('-').ok_or(1)?;
        let p2 = s.find(".jpg").ok_or(2)?;
        Ok(CoverRef {
            year: s[1..p1].parse().map_err(|_| 3)?,
            number: s[p1 + 1..p2].parse().map_err(|_| 4)?,
        })
    }
}

#[allow(clippy::needless_pass_by_value)]
fn cover_image(
    db: PooledPg,
    issue: CoverRef,
) -> Result<impl Reply, Rejection> {
    use crate::schema::covers::dsl as c;
    let data = i::issues
        .inner_join(c::covers)
        .select(c::image)
        .filter(i::year.eq(issue.year))
        .filter(i::number.eq(issue.number))
        .first::<Vec<u8>>(&db)
        .map_err(custom_or_404)?;
    let medium_expires = Utc::now() + Duration::days(90);
    Ok(Response::builder()
        .header(CONTENT_TYPE, IMAGE_JPEG.as_ref())
        .header(EXPIRES, medium_expires.to_rfc2822())
        .body(data))
}

#[allow(clippy::needless_pass_by_value)]
fn frontpage(db: PooledPg) -> Result<impl Reply, Rejection> {
    let n = p::publications
        .select(sql("count(distinct issue)"))
        .filter(not(p::seqno.is_null()))
        .first(&db)
        .map_err(custom)?;

    let years = i::issues
        .select(i::year)
        .distinct()
        .order(i::year)
        .load(&db)
        .map_err(custom)?;

    let all_fa = r::refkeys
        .filter(r::kind.eq(RefKey::FA_ID))
        .order((sql::<Integer>("cast(substr(slug, 1, 2) as int)"), r::slug))
        .load::<IdRefKey>(&db)
        .map_err(custom)?
        .into_iter()
        .map(|rk| rk.refkey)
        .collect::<Vec<_>>();

    let num = 50;
    let (c_def, c) = named(sql("count(*)"), "c");
    let mut titles = t::titles
        .left_join(e::episodes.left_join(ep::episode_parts))
        .select((t::titles::all_columns(), c_def))
        .group_by(t::titles::all_columns())
        .order(c.desc())
        .limit(num)
        .load::<(Title, i64)>(&db)
        .map_err(custom)?
        .into_iter()
        .enumerate()
        .map(|(n, (title, c))| (title, c, (8 * (num - n as i64) / num) as u8))
        .collect::<Vec<_>>();
    titles.sort_by(|a, b| a.0.title.cmp(&b.0.title));

    let (c_def, c) = named(sql("count(*)"), "c");
    let mut refkeys = r::refkeys
        .left_join(er::episode_refkeys.left_join(e::episodes))
        .select((r::refkeys::all_columns(), c_def))
        .filter(r::kind.eq(RefKey::KEY_ID))
        .group_by(r::refkeys::all_columns())
        .order(c.desc())
        .limit(num)
        .load::<(IdRefKey, i64)>(&db)
        .map_err(custom)?
        .into_iter()
        .enumerate()
        .map(|(n, (rk, c))| {
            (rk.refkey, c, (8 * (num - n as i64) / num) as u8)
        })
        .collect::<Vec<_>>();
    refkeys.sort_by(|a, b| a.0.name().cmp(&b.0.name()));

    let (c_ep, c_ep_n) =
        named(sql::<BigInt>("count(distinct episode_id)"), "n");
    let mut creators = c::creators
        .left_join(ca::creator_aliases.left_join(eb::episodes_by))
        .filter(eb::role.eq(any(CreatorSet::MAIN_ROLES)))
        .select((c::creators::all_columns(), c_ep))
        .group_by(c::creators::all_columns())
        .order(c_ep_n.desc())
        .limit(num)
        .load::<(Creator, i64)>(&db)
        .map_err(custom)?
        .into_iter()
        .enumerate()
        .map(|(n, (creator, c))| {
            (creator, c, (8 * (num - n as i64) / num) as u8)
        })
        .collect::<Vec<_>>();
    creators.sort_by(|a, b| a.0.name.cmp(&b.0.name));

    Response::builder().html(|o| {
        templates::frontpage(
            o, n, &all_fa, &years, &titles, &refkeys, &creators,
        )
    })
}

/// Information about an episode / part or article, as published in an issue.
pub struct PublishedInfo {
    pub content: PublishedContent,
    pub seqno: Option<i16>,
    pub classnames: &'static str,
}

pub enum PublishedContent {
    Text {
        article: Article,
        refs: RefKeySet,
        creators: CreatorSet,
    },
    EpisodePart {
        title: Title,
        episode: FullEpisode,
        part: Part,
        best_plac: Option<i16>,
    },
}

pub struct FullEpisode {
    pub episode: Episode,
    pub refs: RefKeySet,
    pub creators: CreatorSet,
    pub published: PartsPublished,
}

impl FullEpisode {
    fn load_details(
        episode: Episode,
        db: &PgConnection,
    ) -> Result<FullEpisode, Error> {
        let refs = RefKeySet::for_episode(&episode, db)?;
        let creators = CreatorSet::for_episode(&episode, db)?;
        let published = PartsPublished::for_episode(&episode, db)?;
        Ok(FullEpisode {
            episode,
            refs,
            creators,
            published,
        })
    }

    fn in_issue(
        episode: Episode,
        issue: &Issue,
        db: &PgConnection,
    ) -> Result<FullEpisode, Error> {
        let refs = RefKeySet::for_episode(&episode, db)?;
        let creators = CreatorSet::for_episode(&episode, db)?;
        let published =
            PartsPublished::for_episode_except(&episode, issue, db)?;
        Ok(FullEpisode {
            episode,
            refs,
            creators,
            published,
        })
    }
}

#[allow(clippy::needless_pass_by_value)]
fn list_year(db: PooledPg, year: u16) -> Result<impl Reply, Rejection> {
    let issues = i::issues
        .filter(i::year.eq(year as i16))
        .order(i::number)
        .load(&db)
        .map_err(custom)?
        .into_iter()
        .map(|issue: Issue| {
            let c_columns = (c::id, ca::name, c::slug);
            let cover_by = c::creators
                .inner_join(ca::creator_aliases.inner_join(cb::covers_by))
                .select(c_columns)
                .filter(cb::issue_id.eq(issue.id))
                .load(&db)?;

            let mut have_main = false;
            let content = p::publications
                .left_outer_join(
                    ep::episode_parts
                        .inner_join(e::episodes.inner_join(t::titles)),
                )
                .left_outer_join(a::articles)
                .select((
                    (
                        t::titles::all_columns(),
                        e::episodes::all_columns(),
                        (ep::id, ep::part_no, ep::part_name),
                    )
                        .nullable(),
                    a::articles::all_columns().nullable(),
                    p::seqno,
                    p::best_plac,
                ))
                .filter(p::issue.eq(issue.id))
                .order(p::seqno)
                .load::<(
                    Option<(Title, Episode, Part)>,
                    Option<Article>,
                    Option<i16>,
                    Option<i16>,
                )>(&db)?
                .into_iter()
                .map(|row| match row {
                    (Some((t, mut e, part)), None, seqno, b) => {
                        let classnames =
                            if e.teaser.is_none() || !part.is_first() {
                                e.teaser = None;
                                "episode noteaser"
                            } else if t.title == "Fantomen" && !have_main {
                                have_main = true;
                                "episode main"
                            } else {
                                "episode"
                            };
                        let content = PublishedContent::EpisodePart {
                            title: t,
                            episode: FullEpisode::in_issue(e, &issue, &db)?,
                            part,
                            best_plac: b,
                        };
                        Ok(PublishedInfo {
                            content,
                            seqno,
                            classnames,
                        })
                    }
                    (None, Some(a), seqno, None) => {
                        let refs = RefKeySet::for_article(&a, &db)?;
                        let creators = CreatorSet::for_article(&a, &db)?;
                        Ok(PublishedInfo {
                            content: PublishedContent::Text {
                                article: a,
                                refs,
                                creators,
                            },
                            seqno,
                            classnames: "article",
                        })
                    }
                    row => panic!("Strange row: {:?}", row),
                })
                .collect::<Result<_, Error>>()?;
            Ok((issue, cover_by, content))
        })
        .collect::<Result<Vec<(Issue, Vec<_>, Vec<_>)>, Error>>()
        .map_err(custom)?;
    if issues.is_empty() {
        return Err(not_found());
    }
    Response::builder().html(|o| templates::year(o, year, &issues))
}

#[allow(clippy::needless_pass_by_value)]
fn list_titles(db: PooledPg) -> Result<impl Reply, Rejection> {
    let all = t::titles
        .left_join(e::episodes.left_join(
            ep::episode_parts.left_join(p::publications.left_join(i::issues)),
        ))
        .select((
            t::titles::all_columns(),
            sql("count(*)"),
            sql::<SmallInt>(&format!("min({})", IssueRef::MAGIC_Q)),
            sql::<SmallInt>(&format!("max({})", IssueRef::MAGIC_Q)),
        ))
        .group_by(t::titles::all_columns())
        .order(t::title)
        .load(&db)
        .map_err(custom)?
        .into_iter()
        .map(|(title, c, first, last)| {
            Ok((
                title,
                c,
                IssueRef::from_magic(first),
                IssueRef::from_magic(last),
            ))
        })
        .collect::<Result<Vec<_>, Rejection>>()?;
    Response::builder().html(|o| templates::titles(o, &all))
}

#[allow(clippy::needless_pass_by_value)]
fn one_title(db: PooledPg, slug: String) -> Result<impl Reply, Rejection> {
    let title = t::titles
        .filter(t::slug.eq(slug))
        .first::<Title>(&db)
        .map_err(custom_or_404)?;

    let articles = a::articles
        .select(a::articles::all_columns())
        .left_join(ar::article_refkeys.left_join(r::refkeys))
        .filter(r::kind.eq(RefKey::TITLE_ID))
        .filter(r::slug.eq(&title.slug))
        .inner_join(p::publications.inner_join(i::issues))
        .order(min(sortable_issue()))
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

    let episodes = e::episodes
        .filter(e::title.eq(title.id))
        .select(crate::schema::episodes::all_columns)
        .inner_join(
            ep::episode_parts
                .inner_join(p::publications.inner_join(i::issues)),
        )
        .order(min(sql::<SmallInt>("(year-1950)*64 + number")))
        .group_by(crate::schema::episodes::all_columns)
        .load::<Episode>(&db)
        .map_err(custom)?
        .into_iter()
        .map(|episode| FullEpisode::load_details(episode, &db))
        .collect::<Result<Vec<_>, _>>()
        .map_err(custom)?;

    Response::builder()
        .html(|o| templates::title(o, &title, &articles, &episodes))
}

#[allow(clippy::needless_pass_by_value)]
fn list_refs(db: PooledPg) -> Result<impl Reply, Rejection> {
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

fn one_fa(db: PooledPg, slug: String) -> Result<impl Reply, Rejection> {
    one_ref_impl(db, slug, RefKey::FA_ID)
}

fn one_ref(db: PooledPg, slug: String) -> Result<impl Reply, Rejection> {
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
        let target = slug.replace("_", "-").replace(".html", "");
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

#[allow(clippy::needless_pass_by_value)]
fn list_creators(db: PooledPg) -> Result<impl Reply, Rejection> {
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
fn one_creator(db: PooledPg, slug: String) -> Result<impl Reply, Rejection> {
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

fn oldslug_title(
    db: PooledPg,
    slug: String,
) -> Result<impl Reply, Rejection> {
    // Special case:
    if slug == "favicon.ico" {
        use templates::statics::goda_svg;
        return redirect(&format!("/s/{}", goda_svg.name));
    }
    let target = slug.replace("_", "-").replace(".html", "");

    let n = t::titles
        .filter(t::slug.eq(&target))
        .select(count_star())
        .first::<i64>(&db)
        .map_err(custom)?;
    if n == 1 {
        return redirect(&format!("/titles/{}", target));
    }
    let target = t::titles
        .filter(
            t::slug.ilike(
                &target
                    .replace("-", "")
                    .chars()
                    .map(|c| c.to_string())
                    .collect::<Vec<_>>()
                    .join("%"),
            ),
        )
        .select(t::slug)
        .first::<String>(&db)
        .map_err(custom_or_404)?;
    redirect(&format!("/titles/{}", target))
}

fn custom_or_404(e: diesel::result::Error) -> Rejection {
    match e {
        diesel::result::Error::NotFound => not_found(),
        e => custom(e),
    }
}

fn redirect(url: &str) -> Result<Response<Vec<u8>>, Rejection> {
    use warp::http::header::LOCATION;
    use warp::http::status::StatusCode;
    let msg = format!("Try {:?}", url);
    Response::builder()
        .status(StatusCode::PERMANENT_REDIRECT)
        .header(LOCATION, url)
        .body(msg.into_bytes())
        .map_err(custom)
}

fn customize_error(err: Rejection) -> Result<impl Reply, Rejection> {
    match err.status() {
        StatusCode::NOT_FOUND => {
            eprintln!("Got a 404: {:?}", err);
            // We have a custom 404 page!
            Response::builder()
                .status(StatusCode::NOT_FOUND)
                .html(|o| templates::notfound(o, StatusCode::NOT_FOUND))
        }
        code => {
            eprintln!("Got a {}: {:?}", code.as_u16(), err);
            Response::builder()
                .status(code)
                .html(|o| templates::error(o, code))
        }
    }
}

use diesel::expression::SqlLiteral;
use diesel::sql_types::SmallInt;

fn named<T>(
    query: SqlLiteral<T>,
    name: &str,
) -> (SqlLiteral<T, SqlLiteral<T>>, SqlLiteral<T>) {
    use diesel::dsl::sql;
    (query.sql(&format!(" {}", name)), sql::<T>(name))
}

fn sortable_issue() -> SqlLiteral<SmallInt> {
    use diesel::dsl::sql;
    sql("(year-1950)*64 + number")
}
