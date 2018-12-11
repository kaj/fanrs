#[macro_use]
extern crate diesel;

mod listissues;
mod models;
mod readfiles;
mod schema;
mod server;

use crate::listissues::list_issues;
use diesel::pg::PgConnection;
use diesel::prelude::*;
use dotenv::dotenv;
use failure::format_err;
use std::env;
use std::process::exit;
use structopt::StructOpt;

#[derive(StructOpt)]
#[structopt(name = "fanrs", about = "Manage index of the Phantom comics")]
enum Fanrs {
    #[structopt(name = "readfiles")]
    /// Read data from xml content files.
    ReadFiles {
        #[structopt(name = "year")]
        /// Year(s) to read data for.
        years: Vec<u32>,
    },
    #[structopt(name = "listissues")]
    /// List known comic book issues (in compact format).
    ListIssues,

    #[structopt(name = "runserver")]
    /// Run the web server.
    RunServer,
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
        Fanrs::ReadFiles { years } => {
            if years.is_empty() {
                return Err(format_err!(
                    "No year(s) to read files for given."
                ));
            }
            for year in years {
                readfiles::load_year(year as i16, &db)?;
            }
            readfiles::delete_unpublished(&db)?;
            Ok(())
        }
        Fanrs::ListIssues => list_issues(&db),
        Fanrs::RunServer => server::run(&db_url),
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
