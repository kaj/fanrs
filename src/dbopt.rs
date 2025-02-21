use diesel::prelude::*;
use diesel_async::pooled_connection::AsyncDieselConnectionManager;
use diesel_async::pooled_connection::deadpool::{BuildError, Pool};
use diesel_async::{AsyncConnection, AsyncPgConnection};

/// An asynchronous postgres database connection pool.
pub type PgPool = Pool<AsyncPgConnection>;

#[derive(clap::Parser)]
pub struct DbOpt {
    /// How to connect to the postgres database.
    #[clap(long, env = "DATABASE_URL", hide_env_values = true)]
    db_url: String,
}

impl DbOpt {
    /// Get a single database connection from the configured url.
    pub async fn get_db(&self) -> Result<AsyncPgConnection, ConnectionError> {
        AsyncPgConnection::establish(&self.db_url).await
    }

    /// Get a database connection pool from the configured url.
    pub fn get_pool(&self) -> Result<PgPool, BuildError> {
        let config = AsyncDieselConnectionManager::new(&self.db_url);
        PgPool::builder(config).build()
    }
}
