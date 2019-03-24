mod covers;
mod creators;
mod paginator;
mod publist;
mod refs;
pub mod search;
mod titles;

use self::covers::{cover_image, redirect_cover};
pub use self::creators::CoverSet;
pub use self::paginator::Paginator;
pub use self::publist::{OtherContribs, PartsPublished};
use self::refs::{get_all_fa, one_fa};
use self::search::{search, search_autocomplete};

use crate::models::{
    Article, Creator, CreatorSet, Episode, Issue, OtherMag, Part, RefKey,
    RefKeySet, Title,
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
use crate::templates::{self, RenderRucte, ToHtml};
use chrono::{Duration, Utc};
use diesel::dsl::{not, sql};
use diesel::expression::SqlLiteral;
use diesel::pg::PgConnection;
use diesel::prelude::*;
use diesel::r2d2::{ConnectionManager, Pool, PooledConnection};
use diesel::sql_types::SmallInt;
use diesel::QueryDsl;
use failure::Error;
use mime::TEXT_PLAIN;
use std::io::{self, Write};
use warp::filters::BoxedFilter;
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
type PgFilter = BoxedFilter<(PooledPg,)>;

/// Get or head - a filter matching GET and HEAD requests only.
fn goh() -> BoxedFilter<()> {
    use warp::{get2 as get, head};
    get().or(head()).unify().boxed()
}

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
    use warp::{path, path::end, path::param, path::tail};
    let routes = warp::any()
        .and(goh().and(path("s")).and(tail()).and_then(static_file))
        .or(goh()
            .and(path("c"))
            .and(s())
            .and(param())
            .and(end())
            .and_then(cover_image))
        .or(goh().and(end()).and(s()).and_then(frontpage))
        .or(goh()
            .and(path("search"))
            .and(end())
            .and(s())
            .and(query())
            .and_then(search))
        .or(goh()
            .and(path("ac"))
            .and(end())
            .and(s())
            .and(query())
            .and_then(search_autocomplete))
        .or(path("titles").and(titles::routes(s())))
        .or(goh()
            .and(path("fa"))
            .and(s())
            .and(param())
            .and(end())
            .and_then(one_fa))
        .or(path("what").and(refs::what_routes(s())))
        .or(path("who").and(creators::routes(s())))
        .or(goh()
            .and(path("static"))
            .and(s())
            .and(param())
            .and(param())
            .and(end())
            .and_then(redirect_cover))
        .or(goh()
            .and(path("robots.txt"))
            .and(end())
            .and_then(robots_txt))
        .or(goh().and(s()).and(param()).and(end()).and_then(list_year))
        .or(goh()
            .and(s())
            .and(param())
            .and(end())
            .and_then(titles::oldslug))
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

fn robots_txt() -> Result<impl Reply, Rejection> {
    Ok(Response::builder()
        .header(CONTENT_TYPE, TEXT_PLAIN.as_ref())
        .body("User-agent: *\nDisallow: /search\nDisallow: /ac\n"))
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
    let titles = Title::cloud(num, &db).map_err(custom)?;
    let refkeys = RefKey::cloud(num, &db).map_err(custom)?;
    let creators = Creator::cloud(num, &db).map_err(custom)?;

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
        label: String,
    },
}

pub struct FullEpisode {
    pub episode: Episode,
    pub refs: RefKeySet,
    pub creators: CreatorSet,
    pub published: PartsPublished,
    pub orig_mag: Option<OtherMag>,
}

impl FullEpisode {
    fn load_details(
        episode: Episode,
        db: &PgConnection,
    ) -> Result<FullEpisode, Error> {
        let refs = RefKeySet::for_episode(&episode, db)?;
        let creators = CreatorSet::for_episode(&episode, db)?;
        let published = PartsPublished::for_episode(&episode, db)?;
        let orig_mag = episode
            .orig_mag_id
            .map(|id| OtherMag::get_by_id(id, db))
            .transpose()?;
        Ok(FullEpisode {
            episode,
            refs,
            creators,
            published,
            orig_mag,
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
        let orig_mag = episode
            .orig_mag_id
            .map(|id| OtherMag::get_by_id(id, db))
            .transpose()?;
        Ok(FullEpisode {
            episode,
            refs,
            creators,
            published,
            orig_mag,
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
                    p::label,
                ))
                .filter(p::issue.eq(issue.id))
                .order(p::seqno)
                .load::<(
                    Option<(Title, Episode, Part)>,
                    Option<Article>,
                    Option<i16>,
                    Option<i16>,
                    String,
                )>(&db)?
                .into_iter()
                .map(|row| match row {
                    (Some((t, mut e, part)), None, seqno, b, label) => {
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
                            label,
                        };
                        Ok(PublishedInfo {
                            content,
                            seqno,
                            classnames,
                        })
                    }
                    (None, Some(a), seqno, None, _label) => {
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
    let years = i::issues
        .select((sql::<SmallInt>("min(year)"), sql::<SmallInt>("max(year)")))
        .first::<(i16, i16)>(&db)
        .map_err(custom)?;
    let years = YearLinks::new(years.0 as u16, year, years.1 as u16);
    Response::builder().html(|o| templates::year(o, year, &years, &issues))
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

fn sortable_issue() -> SqlLiteral<SmallInt> {
    use diesel::dsl::sql;
    sql("(year-1950)*64 + number")
}

pub struct YearLinks {
    first: u16,
    shown: u16,
    last: u16,
}

impl YearLinks {
    fn new(first: u16, shown: u16, last: u16) -> Self {
        YearLinks { first, shown, last }
    }
}

impl ToHtml for YearLinks {
    fn to_html(&self, out: &mut Write) -> io::Result<()> {
        let shown = self.shown;
        let one = |out: &mut Write, y: u16| -> io::Result<()> {
            if y == shown {
                write!(out, "<b>{}</b>", y)?;
            } else {
                write!(out, "<a href='/{}'>{}</a>", y, y)?;
            }
            Ok(())
        };
        let from = if self.shown > self.first + 7 {
            self.shown - 5
        } else {
            self.first
        };
        let to = if self.shown + 7 < self.last {
            self.shown + 5
        } else {
            self.last
        };
        if from > self.first {
            one(out, self.first)?;
            write!(out, " … ")?;
        }
        one(out, from)?;
        for y in from + 1..=to {
            write!(out, ", ")?;
            one(out, y)?;
        }
        if to < self.last {
            write!(out, " … ")?;
            one(out, self.last)?;
        }
        Ok(())
    }
}
