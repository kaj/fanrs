#[macro_use]
extern crate diesel;

mod fetchcovers;
mod listissues;
mod models;
mod readfiles;
mod schema;
mod server;

use crate::fetchcovers::fetch_covers;
use crate::listissues::list_issues;
use diesel::pg::PgConnection;
use diesel::prelude::*;
use dotenv::dotenv;
use failure::format_err;
use std::env;
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
enum Fanrs {
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
    let db_url = env::var("DATABASE_URL")?;
    let db = PgConnection::establish(&db_url)?;

    match opt {
        Fanrs::ReadFiles {
            basedir,
            all,
            years,
        } => {
            if all {
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
                    readfiles::load_year(&basedir, year as i16, &db)?;
                }
            }
            readfiles::delete_unpublished(&db)?;
            Ok(())
        }
        Fanrs::ListIssues => list_issues(&db),
        Fanrs::RunServer => server::run(&db_url),
        Fanrs::FetchCovers => fetch_covers(&db),
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
