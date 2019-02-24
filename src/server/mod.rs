mod covers;
mod creators;
mod publist;
mod refs;
mod render_ructe;
pub mod search;
mod titles;

use self::covers::{cover_image, redirect_cover};
use self::creators::{creator_cloud, list_creators, one_creator};
pub use self::publist::PartsPublished;
use self::refs::{get_all_fa, list_refs, one_fa, one_ref, refkey_cloud};
use self::render_ructe::RenderRucte;
use self::search::{search, search_autocomplete};
use self::titles::{list_titles, oldslug_title, one_title, title_cloud};

use crate::models::{
    Article, CreatorSet, Episode, Issue, Part, RefKeySet, Title,
};
use crate::schema::articles::dsl as a;
use crate::schema::covers_by::dsl as cb;
use crate::schema::creator_aliases::dsl as ca;
use crate::schema::creators::dsl as c;
use crate::schema::episode_parts::dsl as ep;
use crate::schema::episodes::dsl as e;
use crate::schema::issues::dsl as i;
use crate::schema::publications::dsl as p;
use crate::schema::titles::dsl as t;
use crate::templates;
use chrono::{Duration, Utc};
use diesel::dsl::{not, sql};
use diesel::pg::PgConnection;
use diesel::prelude::*;
use diesel::r2d2::{ConnectionManager, Pool, PooledConnection};
use diesel::QueryDsl;
use failure::Error;
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
            .and(path("static"))
            .and(s())
            .and(path::param())
            .and(path::param())
            .and(end())
            .and_then(redirect_cover))
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

    let all_fa = get_all_fa(&db).map_err(custom)?;

    let num = 50;
    let titles = title_cloud(num, &db).map_err(custom)?;
    let refkeys = refkey_cloud(num, &db).map_err(custom)?;
    let creators = creator_cloud(num, &db).map_err(custom)?;

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
