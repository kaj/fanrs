mod render_ructe;

use self::render_ructe::RenderRucte;
use diesel::pg::PgConnection;
use diesel::prelude::*;
use diesel::r2d2::{ConnectionManager, Pool, PooledConnection};
use templates;
use warp::http::Response;
use warp::{self, reject, Filter, Rejection, Reply};

type PooledPg = PooledConnection<ConnectionManager<PgConnection>>;
type PgPool = Pool<ConnectionManager<PgConnection>>;

pub fn run(db_url: &str) -> Result<(), ()> {
    let pool = pg_pool(db_url);
    let s = warp::any()
        .and_then(move || match pool.get() {
            Ok(conn) => Ok(conn),
            Err(e) => {
                eprintln!("Failed to get a db connection: {}", e);
                Err(reject::server_error())
            }
        })
        .boxed();
    use warp::{get2 as get, index, path};
    let routes = warp::any().and(
        get()
            .and(path("titles"))
            .and(index())
            .and(s)
            .and_then(list_titles),
    );
    warp::serve(routes).run(([127, 0, 0, 1], 1536));
    Ok(())
}

fn pg_pool(database_url: &str) -> PgPool {
    let manager = ConnectionManager::<PgConnection>::new(database_url);
    Pool::new(manager).expect("Postgres connection pool could not be created")
}

fn list_titles(db: PooledPg) -> Result<impl Reply, Rejection> {
    use models::Title;
    use schema::titles::dsl;
    let all = dsl::titles
        .load::<Title>(&db)
        .map_err(|_| reject::server_error())?;
    Response::builder().html(|o| templates::titles(o, &all))
}
