#![recursion_limit = "128"]
#[macro_use]
extern crate diesel;

mod checkstrips;
mod count_pages;
mod dbopt;
mod fetchcovers;
mod listissues;
mod models;
mod readfiles;
mod schema;
mod server;

use crate::checkstrips::check_strips;
use crate::listissues::list_issues;
use anyhow::{Context, Result};
use clap::Parser;
use dbopt::DbOpt;
use dotenv::dotenv;

#[derive(clap::Parser)]
#[structopt(about, author)]
enum Fanrs {
    /// Read data from xml content files.
    ReadFiles(readfiles::Args),

    /// List known comic book issues (in compact format).
    ListIssues(DbOpt),

    /// Run the web server.
    RunServer(server::Args),

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
    async fn run(self) -> Result<()> {
        match self {
            Fanrs::ReadFiles(args) => args.run(),
            Fanrs::ListIssues(db) => list_issues(&db.get_db()?),
            Fanrs::RunServer(args) => {
                args.run().await;
                Ok(())
            }
            Fanrs::FetchCovers(args) => args.run().await,
            Fanrs::CheckStrips(db) => check_strips(&db.get_db()?),
            Fanrs::CountPages(args) => args.run(),
        }
    }
}

#[tokio::main(flavor = "multi_thread", worker_threads = 10)]
async fn main() -> Result<()> {
    match dotenv() {
        Ok(_) => (),
        Err(ref err) if err.not_found() => (),
        Err(e) => return Err(e).context("Failed to read .env"),
    }
    env_logger::init();
    Fanrs::parse().run().await
}

include!(concat!(env!("OUT_DIR"), "/templates.rs"));
