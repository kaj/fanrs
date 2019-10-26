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
use crate::fetchcovers::fetch_covers;
use crate::listissues::list_issues;
use diesel::pg::PgConnection;
use diesel::prelude::*;
use dotenv::dotenv;
use failure::format_err;
use std::process::exit;
use structopt::StructOpt;

#[derive(StructOpt)]
#[structopt(about, author)]
struct Fanrs {
    /// How to connect to the postgres database.
    #[structopt(long, env = "DATABASE_URL", hide_env_values = true)]
    db_url: String,

    #[structopt(subcommand)]
    cmd: Command,
}

#[derive(StructOpt)]
enum Command {
    /// Read data from xml content files.
    ReadFiles(readfiles::Args),

    /// List known comic book issues (in compact format).
    ListIssues,

    /// Run the web server.
    RunServer,

    /// Fetch missing cover images from phantomwiki.
    FetchCovers,

    /// Check assumptions about which titles has daystrips and/or sunday pages.
    ///
    /// The code contains hardcoded lists of which comics has
    /// daystrips or sunday pages, this routine checks that those
    /// assumptions are correct with the database values.
    CheckStrips,

    /// Calculate number of pages from a yearbook toc.
    CountPages(count_pages::CountPages),
}

impl Fanrs {
    fn get_db(&self) -> Result<PgConnection, failure::Error> {
        PgConnection::establish(&self.db_url).map_err(|e| {
            format_err!("Failed to establish postgres connection: {}", e)
        })
    }
}

fn main() {
    match dotenv() {
        Ok(_) => (),
        Err(ref err) if err.not_found() => (),
        Err(err) => {
            eprintln!("Failed to read env: {}", err);
            exit(1);
        }
    }
    match run() {
        Ok(()) => (),
        Err(error) => {
            eprintln!("Error: {}", error);
            exit(1);
        }
    }
}

fn run() -> Result<(), failure::Error> {
    let opt = Fanrs::from_args();

    match opt.cmd {
        Command::ReadFiles(ref args) => args.run(&opt.get_db()?),
        Command::ListIssues => Ok(list_issues(&opt.get_db()?)?),
        Command::RunServer => server::run(&opt.db_url),
        Command::FetchCovers => fetch_covers(&opt.get_db()?),
        Command::CheckStrips => check_strips(&opt.get_db()?),
        Command::CountPages(args) => Ok(args.run()),
    }
}

include!(concat!(env!("OUT_DIR"), "/templates.rs"));
