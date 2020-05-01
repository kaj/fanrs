use diesel::pg::PgConnection;
use diesel::prelude::*;
use diesel::r2d2::{ConnectionManager, Pool, PoolError};
use std::time::Duration;
use structopt::StructOpt;

pub type PgPool = Pool<ConnectionManager<PgConnection>>;

#[derive(StructOpt)]
pub struct DbOpt {
    /// How to connect to the postgres database.
    #[structopt(long, env = "DATABASE_URL", hide_env_values = true)]
    db_url: String,
}

impl DbOpt {
    /// Get a single database connection from the configured url.
    pub fn get_db(&self) -> Result<PgConnection, ConnectionError> {
        PgConnection::establish(&self.db_url)
    }

    /// Get a database connection pool from the configured url.
    pub fn get_pool(&self) -> Result<PgPool, PoolError> {
        Pool::builder()
            .test_on_check_out(false)
            .connection_timeout(Duration::from_millis(500))
            .build(ConnectionManager::new(&self.db_url))
    }
}
