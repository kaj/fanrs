mod render_ructe;

use self::render_ructe::RenderRucte;
use chrono::{Duration, Utc};
use diesel::pg::PgConnection;
use diesel::prelude::*;
use diesel::r2d2::{ConnectionManager, Pool, PooledConnection};
use diesel::QueryDsl;
use failure::Error;
use models::{Episode, Issue, Part, RefKey, Title};
use templates;
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
fn static_file(name: Tail) -> Result<impl Reply, Rejection> {
    use templates::statics::StaticFile;
    if let Some(data) = StaticFile::get(name.as_str()) {
        let far_expires = Utc::now() + Duration::days(180);
        Ok(Response::builder()
            //.status(StatusCode::OK)
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

fn frontpage(db: PooledPg) -> Result<impl Reply, Rejection> {
    use schema::issues::dsl;
    let years = dsl::issues
        .select(dsl::year)
        .distinct()
        .order(dsl::year)
        .load(&db)
        .map_err(custom)?;
    eprintln!("Years: {:?}", years);
    Response::builder().html(|o| templates::frontpage(o, &years))
}

/// Information about an episode / part or article, as published in an issue.
pub enum PublishedInfo {
    EpisodePart {
        title: Title,
        episode: Episode,
        refs: Vec<RefKey>,
        part: Part,
        seqno: Option<i16>,
        best_plac: Option<i16>,
    },
}

fn list_year(db: PooledPg, year: u16) -> Result<impl Reply, Rejection> {
    use schema::issues::dsl;
    let issues = dsl::issues
        .filter(dsl::year.eq(year as i16))
        .load(&db)
        .map_err(custom)?
        .into_iter()
        .map(|issue: Issue| {
            use schema::episode_parts::dsl as ep;
            use schema::episodes::dsl as e;
            use schema::publications::dsl as p;
            use schema::titles::dsl as t;
            let id = issue.id;
            (
                issue,
                t::titles
                    .inner_join(e::episodes.inner_join(
                        ep::episode_parts.inner_join(p::publications),
                    ))
                    .select((
                        t::titles::all_columns(),
                        e::episodes::all_columns(),
                        (ep::part_no, ep::part_name),
                        p::seqno,
                        p::best_plac,
                    ))
                    .filter(p::issue.eq(id))
                    .order(p::seqno)
                    .load::<(Title, Episode, Part, Option<i16>, Option<i16>)>(
                        &db,
                    )
                    .unwrap()
                    .into_iter()
                    .map(|(t, e, p, s, b)| {
                        let refkeys = e.load_refs(&db).unwrap();
                        PublishedInfo::EpisodePart {
                            title: t,
                            episode: e,
                            refs: refkeys,
                            part: p,
                            seqno: s,
                            best_plac: b,
                        }
                    })
                    .collect(),
            )
        })
        .collect::<Vec<(Issue, Vec<_>)>>();

    Response::builder().html(|o| templates::year(o, year, &issues))
}
fn list_titles(db: PooledPg) -> Result<impl Reply, Rejection> {
    use schema::titles::dsl;
    let all = dsl::titles.load::<Title>(&db).map_err(custom)?;
    Response::builder().html(|o| templates::titles(o, &all))
}

fn one_title(db: PooledPg, tslug: String) -> Result<impl Reply, Rejection> {
    use schema::episodes::dsl::{episodes, title};
    use schema::titles::dsl::{slug, titles};
    let t = titles
        .filter(slug.eq(tslug))
        .first::<Title>(&db)
        .map_err(custom)?;
    let all = episodes
        .filter(title.eq(t.id))
        .load::<Episode>(&db)
        .map_err(custom)?;
    Response::builder().html(|o| templates::title(o, &t, &all))
}