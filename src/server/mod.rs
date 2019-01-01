mod render_ructe;

use self::render_ructe::RenderRucte;
use crate::models::{
    Article, Creator, CreatorSet, Episode, IdRefKey, Issue, IssueRef, Part,
    PartInIssue, RefKey, RefKeySet, Title,
};
use crate::templates;
use chrono::{Duration, Utc};
use diesel::pg::PgConnection;
use diesel::prelude::*;
use diesel::r2d2::{ConnectionManager, Pool, PooledConnection};
use diesel::QueryDsl;
use failure::Error;
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
            .and_then(list_year));
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

use std::str::FromStr;
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
    use crate::schema::issues::dsl as i;
    use mime::IMAGE_JPEG;
    let data = i::issues
        .inner_join(c::covers)
        .select(c::image)
        .filter(i::year.eq(issue.year))
        .filter(i::number.eq(issue.number))
        .first::<Vec<u8>>(&db)
        .map_err(|e| match e {
            diesel::result::Error::NotFound => not_found(),
            e => custom(e),
        })?;
    let medium_expires = Utc::now() + Duration::days(90);
    Ok(Response::builder()
        .header(CONTENT_TYPE, IMAGE_JPEG.as_ref())
        .header(EXPIRES, medium_expires.to_rfc2822())
        .body(data))
}

#[allow(clippy::needless_pass_by_value)]
fn frontpage(db: PooledPg) -> Result<impl Reply, Rejection> {
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
    use diesel::dsl::{any, sql};
    use diesel::sql_types::{BigInt, Integer};

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
    let mut titles = t::titles
        .left_join(e::episodes.left_join(
            ep::episode_parts.left_join(p::publications.left_join(i::issues)),
        ))
        .select((t::titles::all_columns(), sql::<BigInt>("count(*) c")))
        .group_by(t::titles::all_columns())
        .order(sql::<BigInt>("c").desc())
        .limit(num)
        .load::<(Title, i64)>(&db)
        .map_err(custom)?
        .into_iter()
        .enumerate()
        .map(|(n, (title, c))| (title, c, (8 * (num - n as i64) / num) as u8))
        .collect::<Vec<_>>();
    titles.sort_by(|a, b| a.0.title.cmp(&b.0.title));

    let mut refkeys = r::refkeys
        .left_join(er::episode_refkeys.left_join(e::episodes.left_join(
            ep::episode_parts.left_join(p::publications.left_join(i::issues)),
        )))
        .select((r::refkeys::all_columns(), sql::<BigInt>("count(*) c")))
        .filter(r::kind.eq(RefKey::KEY_ID))
        .group_by(r::refkeys::all_columns())
        .order(sql::<BigInt>("c").desc())
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

    let main_roles = vec!["by", "bild", "text"];
    let mut creators = c::creators
        .left_join(ca::creator_aliases.left_join(eb::episodes_by))
        .filter(eb::role.eq(any(&main_roles)))
        .select((
            c::creators::all_columns(),
            sql::<BigInt>("count(distinct episode_id) c"),
        ))
        .group_by(c::creators::all_columns())
        .order(sql::<BigInt>("c").desc())
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
        templates::frontpage(o, &all_fa, &years, &titles, &refkeys, &creators)
    })
}

/// Information about an episode / part or article, as published in an issue.
pub struct PublishedInfo {
    pub content: PublishedContent,
    pub creators: CreatorSet,
    pub refs: RefKeySet,
    pub seqno: Option<i16>,
    pub classnames: &'static str,
}

pub enum PublishedContent {
    Text {
        title: String,
        subtitle: Option<String>,
        note: Option<String>,
    },
    EpisodePart {
        title: Title,
        episode: Episode,
        part: Part,
        published: Vec<PartInIssue>,
        best_plac: Option<i16>,
    },
}

#[allow(clippy::needless_pass_by_value)]
fn list_year(db: PooledPg, year: u16) -> Result<impl Reply, Rejection> {
    use crate::schema::issues::dsl as i;
    let issues = i::issues
        .filter(i::year.eq(year as i16))
        .order(i::number)
        .load(&db)
        .map_err(custom)?
        .into_iter()
        .map(|issue: Issue| {
            use crate::schema::articles::dsl as a;
            use crate::schema::covers_by::dsl as cb;
            use crate::schema::creator_aliases::dsl as ca;
            use crate::schema::creators::dsl as c;
            use crate::schema::episode_parts::dsl as ep;
            use crate::schema::episodes::dsl as e;
            use crate::schema::publications::dsl as p;
            use crate::schema::titles::dsl as t;
            let issue_id = issue.id;
            let c_columns = (c::id, ca::name, c::slug);
            (
                issue,
                c::creators
                    .inner_join(ca::creator_aliases.inner_join(cb::covers_by))
                    .select(c_columns)
                    .filter(cb::issue_id.eq(issue_id))
                    .load(&db)
                    .unwrap(),
                p::publications
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
                    .filter(p::issue.eq(issue_id))
                    .order(p::seqno)
                    .load::<(
                        Option<(Title, Episode, Part)>,
                        Option<Article>,
                        Option<i16>,
                        Option<i16>,
                    )>(&db)
                    .unwrap()
                    .into_iter()
                    .map(|row| match row {
                        (Some((t, e, part)), None, seqno, b) => {
                            let refs =
                                RefKeySet::for_episode(&e, &db).unwrap();
                            let classnames = if t.title == "Fantomen" {
                                "episode main"
                            } else if e.teaser.is_none() {
                                "episode noteaser"
                            } else {
                                "episode"
                            };
                            let creators =
                                CreatorSet::for_episode(&e, &db).unwrap();
                            let published = i::issues
                                .inner_join(
                                    p::publications
                                        .inner_join(ep::episode_parts),
                                )
                                .select((
                                    (i::year, i::number, i::number_str),
                                    (ep::id, ep::part_no, ep::part_name),
                                ))
                                .filter(ep::episode.eq(e.id))
                                .filter(i::id.ne(issue_id))
                                .load(&db)
                                .unwrap();
                            PublishedInfo {
                                content: PublishedContent::EpisodePart {
                                    title: t,
                                    episode: e,
                                    part,
                                    published,
                                    best_plac: b,
                                },
                                creators,
                                refs,
                                seqno,
                                classnames,
                            }
                        }
                        (None, Some(a), seqno, None) => {
                            let refs =
                                RefKeySet::for_article(&a, &db).unwrap();
                            let creators =
                                CreatorSet::for_article(&a, &db).unwrap();
                            let Article {
                                title,
                                subtitle,
                                note,
                                ..
                            } = a;
                            PublishedInfo {
                                content: PublishedContent::Text {
                                    title,
                                    subtitle,
                                    note,
                                },
                                creators,
                                refs,
                                seqno,
                                classnames: "article",
                            }
                        }
                        row => panic!("Strange row: {:?}", row),
                    })
                    .collect(),
            )
        })
        .collect::<Vec<(Issue, Vec<_>, Vec<_>)>>();

    Response::builder().html(|o| templates::year(o, year, &issues))
}

#[allow(clippy::needless_pass_by_value)]
fn list_titles(db: PooledPg) -> Result<impl Reply, Rejection> {
    use crate::schema::episode_parts::dsl as ep;
    use crate::schema::episodes::dsl as e;
    use crate::schema::issues::dsl as i;
    use crate::schema::publications::dsl as p;
    use crate::schema::titles::dsl as t;
    use diesel::dsl::sql;
    use diesel::sql_types::{BigInt, Text};

    let all = t::titles
        .left_join(e::episodes.left_join(
            ep::episode_parts.left_join(p::publications.left_join(i::issues)),
        ))
        .select((
            t::titles::all_columns(),
            sql::<BigInt>("count(*)"),
            sql::<Text>("min(concat(year, ' ', number_str))"),
            sql::<Text>("max(concat(year, ' ', number_str))"),
        ))
        .group_by(t::titles::all_columns())
        .order(t::title)
        .load::<(Title, i64, String, String)>(&db)
        .map_err(custom)?
        .into_iter()
        .map(|(title, c, first, last)| {
            Ok((
                title,
                c,
                first.parse().map_err(custom)?,
                last.parse().map_err(custom)?,
            ))
        })
        .collect::<Result<Vec<_>, Rejection>>()?;
    Response::builder().html(|o| templates::titles(o, &all))
}

#[allow(clippy::needless_pass_by_value)]
fn one_title(db: PooledPg, tslug: String) -> Result<impl Reply, Rejection> {
    use crate::schema::titles::dsl::{slug, titles};
    let (title, articles, episodes) = titles
        .filter(slug.eq(tslug))
        .first::<Title>(&db)
        .and_then(|title| {
            use crate::schema::article_refkeys::dsl as ar;
            use crate::schema::articles::{all_columns, dsl as a};
            use crate::schema::episode_parts::dsl as ep;
            use crate::schema::episodes::dsl as e;
            use crate::schema::issues::dsl as i;
            use crate::schema::publications::dsl as p;
            use crate::schema::refkeys::dsl as r;
            use diesel::dsl::{min, sql};
            use diesel::sql_types::SmallInt;
            let articles = a::articles
                .select(all_columns)
                .left_join(ar::article_refkeys.left_join(r::refkeys))
                .filter(r::kind.eq(RefKey::TITLE_ID))
                .filter(r::slug.eq(&title.slug))
                .inner_join(p::publications.inner_join(i::issues))
                .order(min(sql::<SmallInt>("(year-1950)*64 + number")))
                .group_by(all_columns)
                .load::<Article>(&db)?
                .into_iter()
                .map(|article| {
                    let refs = RefKeySet::for_article(&article, &db).unwrap();
                    let creators =
                        CreatorSet::for_article(&article, &db).unwrap();
                    let published = i::issues
                        .inner_join(p::publications)
                        .select((i::year, i::number, i::number_str))
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
                .load::<Episode>(&db)?
                .into_iter()
                .map(|episode| {
                    let refs = RefKeySet::for_episode(&episode, &db).unwrap();
                    let creators =
                        CreatorSet::for_episode(&episode, &db).unwrap();
                    let published = i::issues
                        .inner_join(
                            p::publications.inner_join(ep::episode_parts),
                        )
                        .select((
                            (i::year, i::number, i::number_str),
                            (ep::id, ep::part_no, ep::part_name),
                        ))
                        .filter(ep::episode.eq(episode.id))
                        .load::<PartInIssue>(&db)
                        .unwrap();
                    (episode, refs, creators, published)
                })
                .collect::<Vec<_>>();
            Ok((title, articles, episodes))
        })
        .map_err(|e| match e {
            diesel::result::Error::NotFound => not_found(),
            e => custom(e),
        })?;
    Response::builder()
        .html(|o| templates::title(o, &title, &articles, &episodes))
}

#[allow(clippy::needless_pass_by_value)]
fn list_refs(db: PooledPg) -> Result<impl Reply, Rejection> {
    use crate::schema::episode_parts::dsl as ep;
    use crate::schema::episode_refkeys::dsl as er;
    use crate::schema::episodes::dsl as e;
    use crate::schema::issues::dsl as i;
    use crate::schema::publications::dsl as p;
    use crate::schema::refkeys::dsl as r;
    use diesel::dsl::sql;
    use diesel::sql_types::{BigInt, Text};

    let all = r::refkeys
        .filter(r::kind.eq(RefKey::KEY_ID))
        .left_join(er::episode_refkeys.left_join(e::episodes.left_join(
            ep::episode_parts.left_join(p::publications.left_join(i::issues)),
        )))
        .select((
            r::refkeys::all_columns(),
            sql::<BigInt>("count(*)"),
            sql::<Text>("min(concat(year, ' ', number_str))"),
            sql::<Text>("max(concat(year, ' ', number_str))"),
        ))
        .group_by(r::refkeys::all_columns())
        .order(r::title)
        .load::<(IdRefKey, i64, String, String)>(&db)
        .map_err(custom)?
        .into_iter()
        .map(|(refkey, c, first, last)| {
            (refkey.refkey, c, first.parse().ok(), last.parse().ok())
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
    tslug: String,
    kind: i16,
) -> Result<impl Reply, Rejection> {
    use crate::schema::refkeys::dsl as r;
    let (refkey, articles, episodes) = r::refkeys
        .filter(r::kind.eq(kind))
        .filter(r::slug.eq(tslug))
        .first::<IdRefKey>(&db)
        .and_then(|refkey| {
            use crate::schema::article_refkeys::dsl as ar;
            use crate::schema::articles::{all_columns, dsl as a};
            use crate::schema::episode_parts::dsl as ep;
            use crate::schema::episode_refkeys::dsl as er;
            use crate::schema::episodes::dsl as e;
            use crate::schema::issues::dsl as i;
            use crate::schema::publications::dsl as p;
            use crate::schema::refkeys::dsl as r;
            use crate::schema::titles::dsl as t;
            use diesel::dsl::{min, sql};
            use diesel::sql_types::SmallInt;
            let articles = a::articles
                .select(all_columns)
                .left_join(ar::article_refkeys.left_join(r::refkeys))
                .filter(ar::refkey_id.eq(refkey.id))
                .inner_join(p::publications.inner_join(i::issues))
                .order(min(sql::<SmallInt>("(year-1950)*64 + number")))
                .group_by(all_columns)
                .load::<Article>(&db)?
                .into_iter()
                .map(|article| {
                    let refs = RefKeySet::for_article(&article, &db).unwrap();
                    let creators =
                        CreatorSet::for_article(&article, &db).unwrap();
                    let published = i::issues
                        .inner_join(p::publications)
                        .select((i::year, i::number, i::number_str))
                        .filter(p::article_id.eq(article.id))
                        .load::<IssueRef>(&db)
                        .unwrap();
                    (article, refs, creators, published)
                })
                .collect::<Vec<_>>();
            let episodes = e::episodes
                .left_join(er::episode_refkeys)
                .inner_join(t::titles)
                .filter(er::refkey_id.eq(refkey.id))
                .select((
                    t::titles::all_columns(),
                    e::episodes::all_columns(),
                ))
                .inner_join(
                    ep::episode_parts
                        .inner_join(p::publications.inner_join(i::issues)),
                )
                .order(min(sql::<SmallInt>("(year-1950)*64 + number")))
                .group_by((
                    t::titles::all_columns(),
                    e::episodes::all_columns(),
                ))
                .load::<(Title, Episode)>(&db)?
                .into_iter()
                .map(|(title, episode)| {
                    let refs = RefKeySet::for_episode(&episode, &db).unwrap();
                    let creators =
                        CreatorSet::for_episode(&episode, &db).unwrap();
                    let published = i::issues
                        .inner_join(
                            p::publications.inner_join(ep::episode_parts),
                        )
                        .select((
                            (i::year, i::number, i::number_str),
                            (ep::id, ep::part_no, ep::part_name),
                        ))
                        .filter(ep::episode.eq(episode.id))
                        .load::<PartInIssue>(&db)
                        .unwrap();
                    (title, episode, refs, creators, published)
                })
                .collect::<Vec<_>>();
            Ok((refkey.refkey, articles, episodes))
        })
        .map_err(|e| match e {
            diesel::result::Error::NotFound => not_found(),
            e => custom(e),
        })?;
    Response::builder()
        .html(|o| templates::refkey(o, &refkey, &articles, &episodes))
}

#[allow(clippy::needless_pass_by_value)]
fn list_creators(db: PooledPg) -> Result<impl Reply, Rejection> {
    use crate::schema::creator_aliases::dsl as ca;
    use crate::schema::creators::dsl as c;
    use crate::schema::episode_parts::dsl as ep;
    use crate::schema::episodes::dsl as e;
    use crate::schema::episodes_by::dsl as eb;
    use crate::schema::issues::dsl as i;
    use crate::schema::publications::dsl as p;
    use diesel::dsl::sql;
    use diesel::sql_types::{BigInt, Text};

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
            sql::<BigInt>("count(*)"),
            sql::<Text>("min(concat(year, ' ', number_str))"),
            sql::<Text>("max(concat(year, ' ', number_str))"),
        ))
        .group_by(c::creators::all_columns())
        .order(c::name)
        .load::<(Creator, i64, String, String)>(&db)
        .map_err(custom)?
        .into_iter()
        .map(|(creator, c, first, last)| {
            (creator, c, first.parse().ok(), last.parse().ok())
        })
        .collect::<Vec<_>>();
    Response::builder().html(|o| templates::creators(o, &all))
}

#[allow(clippy::needless_pass_by_value)]
fn one_creator(db: PooledPg, slug: String) -> Result<impl Reply, Rejection> {
    use crate::schema::article_refkeys::dsl as ar;
    use crate::schema::articles::{all_columns, dsl as a};
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
    use diesel::dsl::{all, any, min, sql};
    use diesel::sql_types::SmallInt;

    let creator = c::creators
        .filter(c::slug.eq(slug))
        .first::<Creator>(&db)
        .map_err(|e| match e {
            diesel::result::Error::NotFound => not_found(),
            e => custom(e),
        })?;

    let articles = a::articles
        .select(all_columns)
        .left_join(ar::article_refkeys.left_join(r::refkeys))
        .filter(r::kind.eq(RefKey::WHO_ID))
        .filter(r::slug.eq(&creator.slug))
        .inner_join(p::publications.inner_join(i::issues))
        .order(min(sql::<SmallInt>("(year-1950)*64 + number")))
        .group_by(all_columns)
        .load::<Article>(&db)
        .map_err(custom)?
        .into_iter()
        .map(|article| {
            let refs = RefKeySet::for_article(&article, &db).unwrap();
            let creators = CreatorSet::for_article(&article, &db).unwrap();
            let published = i::issues
                .inner_join(p::publications)
                .select((i::year, i::number, i::number_str))
                .filter(p::article_id.eq(article.id))
                .load::<IssueRef>(&db)
                .unwrap();
            (article, refs, creators, published)
        })
        .collect::<Vec<_>>();

    let mut covers = i::issues
        .select(((i::year, i::number, i::number_str), i::cover_best))
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
    let main_roles = vec!["by", "bild", "text", "orig", "ink"];
    let main_episodes = e::episodes
        .inner_join(eb::episodes_by.inner_join(ca::creator_aliases))
        .inner_join(t::titles)
        .filter(ca::creator_id.eq(creator.id))
        .filter(eb::role.eq(any(&main_roles)))
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
        .map(|(title, episode)| {
            let refs = RefKeySet::for_episode(&episode, &db).unwrap();
            let creators = CreatorSet::for_episode(&episode, &db).unwrap();
            let published = i::issues
                .inner_join(p::publications.inner_join(ep::episode_parts))
                .select((
                    (i::year, i::number, i::number_str),
                    (ep::id, ep::part_no, ep::part_name),
                ))
                .filter(ep::episode.eq(episode.id))
                .order((i::year, i::number))
                .load::<PartInIssue>(&db)
                .unwrap();
            (title, episode, refs, creators, published)
        })
        .collect::<Vec<_>>();
    let oe_columns = (t::titles::all_columns(), e::id, e::episode);
    let other_episodes = e::episodes
        .inner_join(eb::episodes_by.inner_join(ca::creator_aliases))
        .inner_join(t::titles)
        .filter(ca::creator_id.eq(creator.id))
        .filter(eb::role.ne(all(&main_roles)))
        .select(oe_columns)
        .inner_join(
            ep::episode_parts
                .inner_join(p::publications.inner_join(i::issues)),
        )
        .order(min(sql::<SmallInt>("(year-1950)*64 + number")))
        .group_by(oe_columns)
        .load::<(Title, i32, Option<String>)>(&db)
        .map_err(custom)?;

    use std::collections::BTreeMap;
    let mut oe: BTreeMap<_, Vec<_>> = BTreeMap::new();
    for (title, episode_id, episode) in other_episodes {
        let published = i::issues
            .inner_join(p::publications.inner_join(ep::episode_parts))
            .select((
                (i::year, i::number, i::number_str),
                (ep::id, ep::part_no, ep::part_name),
            ))
            .filter(ep::episode.eq(episode_id))
            .load::<PartInIssue>(&db)
            .unwrap();
        oe.entry(title).or_default().push((episode, published));
    }

    let o_roles = eb::episodes_by
        .inner_join(ca::creator_aliases)
        .filter(ca::creator_id.eq(creator.id))
        .filter(eb::role.ne(all(&main_roles)))
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
            &articles,
            &covers,
            &all_covers,
            &main_episodes,
            &o_roles,
            &oe,
        )
    })
}
