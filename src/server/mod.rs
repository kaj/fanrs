mod render_ructe;

use self::render_ructe::RenderRucte;
use diesel::pg::PgConnection;
use diesel::prelude::*;
use diesel::r2d2::{ConnectionManager, Pool, PooledConnection};
use failure::Error;
use templates;
use warp::http::Response;
use warp::path::Tail;
use warp::{
    self,
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
        // let _far_expires = SystemTime::now() + FAR;
        Ok(Response::builder()
            //.status(StatusCode::OK)
            .header("content-type", data.mime.as_ref())
            // TODO .header("expires", _far_expires)
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

fn list_year(db: PooledPg, year: u16) -> Result<impl Reply, Rejection> {
    use models::{Episode, Issue, Part, Title};
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
                    ))
                    .filter(p::issue.eq(id))
                    .load::<(Title, Episode, Part)>(&db)
                    .unwrap(),
            )
        })
        .collect::<Vec<(Issue, Vec<_>)>>();

    Response::builder().html(|o| templates::year(o, year, &issues))
}
fn list_titles(db: PooledPg) -> Result<impl Reply, Rejection> {
    use models::Title;
    use schema::titles::dsl;
    let all = dsl::titles.load::<Title>(&db).map_err(custom)?;
    Response::builder().html(|o| templates::titles(o, &all))
}

fn one_title(db: PooledPg, tslug: String) -> Result<impl Reply, Rejection> {
    use models::{Episode, Title};
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
