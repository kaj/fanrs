#![recursion_limit = "128"]
#[macro_use]
extern crate diesel;
#[macro_use]
extern crate serde_derive;

mod checkstrips;
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
use std::path::PathBuf;
use std::process::exit;
use structopt::StructOpt;
use time;

#[derive(StructOpt)]
#[structopt(
    name = "fanrs",
    about = "Manage and serve index of the Phantom comic books.",
    rename_all = "kebab-case"
)]
struct Fanrs {
    /// How to connect to the postgres database.
    #[structopt(long, env = "DATABASE_URL")]
    db_url: String,

    #[structopt(subcommand)]
    cmd: Command,
}

#[derive(StructOpt)]
#[structopt(rename_all = "kebab-case")]
enum Command {
    /// Read data from xml content files.
    ReadFiles {
        /// The directory containing the data files.
        #[structopt(long, short, parse(from_os_str), env = "FANTOMEN_DATA")]
        basedir: PathBuf,

        /// Read data for all years, from 1950 to current.
        #[structopt(long, short)]
        all: bool,

        /// Year(s) to read data for.
        #[structopt(name = "year")]
        years: Vec<u32>,
    },
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
}

impl Fanrs {
    fn get_db(&self) -> Result<PgConnection, failure::Error> {
        PgConnection::establish(&self.db_url).map_err(|e| {
            format_err!("Failed to establish postgres connection: {}", e)
        })
    }
}

fn main() {
    match run() {
        Ok(()) => (),
        Err(error) => {
            eprintln!("Error: {}", error);
            exit(1);
        }
    }
}

fn run() -> Result<(), failure::Error> {
    opt_dotenv()?;
    let opt = Fanrs::from_args();

    match opt.cmd {
        Command::ReadFiles {
            ref basedir,
            ref all,
            ref years,
        } => {
            let db = opt.get_db()?;
            readfiles::read_persondata(&basedir, &db)?;
            if *all {
                let current_year = 1900 + time::now().tm_year as i16;
                for year in 1950..=current_year {
                    readfiles::load_year(&basedir, year, &db)?;
                }
            } else {
                if years.is_empty() {
                    return Err(format_err!(
                        "No year(s) to read files for given."
                    ));
                }
                for year in years {
                    readfiles::load_year(&basedir, *year as i16, &db)?;
                }
            }
            readfiles::delete_unpublished(&db)?;
            Ok(())
        }
        Command::ListIssues => list_issues(&opt.get_db()?),
        Command::RunServer => server::run(&opt.db_url),
        Command::FetchCovers => fetch_covers(&opt.get_db()?),
        Command::CheckStrips => check_strips(&opt.get_db()?),
    }
}

/// Normal dotenv, but if the file .env is not found, that is not an error.
fn opt_dotenv() -> Result<(), dotenv::Error> {
    match dotenv() {
        Ok(_) => Ok(()),
        Err(ref err) if err.not_found() => Ok(()),
        Err(err) => Err(err),
    }
}

include!(concat!(env!("OUT_DIR"), "/templates.rs"));
