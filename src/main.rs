extern crate bigdecimal;
#[macro_use]
extern crate diesel;
extern crate dotenv;
#[macro_use]
extern crate failure;
extern crate mime;
extern crate slug;
#[macro_use]
extern crate structopt;
extern crate warp;
extern crate xmltree;

mod listissues;
mod models;
mod readfiles;
mod schema;
mod server;

use diesel::pg::PgConnection;
use diesel::prelude::*;
use dotenv::dotenv;
use listissues::list_issues;
use std::env;
use std::process::exit;
use structopt::StructOpt;

#[derive(StructOpt)]
#[structopt(name = "fanrs", about = "Manage index of the Phantom comics")]
enum Fanrs {
    #[structopt(name = "readfiles")]
    /// Read data from xml content files.
    ReadFiles { year: u16 },
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
    dotenv()?;
    let db_url = env::var("DATABASE_URL").unwrap();
    let opt = Fanrs::from_args();
    let db = PgConnection::establish(&db_url)?;

    match opt {
        Fanrs::ReadFiles { year } => {
            readfiles::load_year(year as i16, &db)
        }
        Fanrs::ListIssues => {
            list_issues(&db)
        }
        Fanrs::RunServer => {
            server::run(&db_url)
        }
    }
}

include!(concat!(env!("OUT_DIR"), "/templates.rs"));
