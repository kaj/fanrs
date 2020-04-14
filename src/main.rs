#![recursion_limit = "128"]
#[macro_use]
extern crate diesel;

mod checkstrips;
mod count_pages;
mod fetchcovers;
mod listissues;
mod models;
mod readfiles;
mod schema;
mod server;

use crate::checkstrips::check_strips;
use crate::listissues::list_issues;
use diesel::pg::PgConnection;
use diesel::prelude::*;
use dotenv::dotenv;
use failure::{format_err, Error};
use std::process::exit;
use structopt::StructOpt;

#[derive(StructOpt)]
#[structopt(about, author)]
enum Fanrs {
    /// Read data from xml content files.
    ReadFiles(readfiles::Args),

    /// List known comic book issues (in compact format).
    ListIssues(DbOpt),

    /// Run the web server.
    RunServer(DbOpt),

    /// Fetch missing cover images from phantomwiki.
    FetchCovers(fetchcovers::Args),

    /// Check assumptions about which titles has daystrips and/or sunday pages.
    ///
    /// The code contains hardcoded lists of which comics has
    /// daystrips or sunday pages, this routine checks that those
    /// assumptions are correct with the database values.
    CheckStrips(DbOpt),

    /// Calculate number of pages from a yearbook toc.
    CountPages(count_pages::CountPages),
}

impl Fanrs {
    async fn run(self) -> Result<(), Error> {
        match self {
            Fanrs::ReadFiles(args) => args.run(),
            Fanrs::ListIssues(db) => Ok(list_issues(&db.get_db()?)?),
            Fanrs::RunServer(db) => Ok(server::run(&db.db_url).await?),
            Fanrs::FetchCovers(args) => args.run().await,
            Fanrs::CheckStrips(db) => check_strips(&db.get_db()?),
            Fanrs::CountPages(args) => args.run(),
        }
    }
}

#[derive(StructOpt)]
struct DbOpt {
    /// How to connect to the postgres database.
    #[structopt(long, env = "DATABASE_URL", hide_env_values = true)]
    db_url: String,
}

impl DbOpt {
    fn get_db(&self) -> Result<PgConnection, Error> {
        PgConnection::establish(&self.db_url).map_err(|e| {
            format_err!("Failed to establish postgres connection: {}", e)
        })
    }
}

#[tokio::main]
async fn main() {
    match dotenv() {
        Ok(_) => (),
        Err(ref err) if err.not_found() => (),
        Err(err) => {
            eprintln!("Failed to read env: {}", err);
            exit(1);
        }
    }
    env_logger::init();
    match Fanrs::from_args().run().await {
        Ok(()) => (),
        Err(error) => {
            eprintln!("Error: {}", error);
            exit(1);
        }
    }
}

include!(concat!(env!("OUT_DIR"), "/templates.rs"));
