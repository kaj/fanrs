use crate::schema::covers::dsl as c;
use crate::schema::issues::dsl as i;
use crate::DbOpt;
use diesel::dsl::now;
use diesel::pg::upsert::excluded;
use diesel::pg::PgConnection;
use diesel::prelude::*;
use failure::{format_err, Error};
use reqwest::{Client, Response};
use scraper::{Html, Selector};
use structopt::StructOpt;

#[derive(StructOpt)]
pub struct Args {
    #[structopt(flatten)]
    db: DbOpt,

    /// No operation, only check which covers would be fetched.
    #[structopt(long)]
    no_op: bool,

    /// Update some of the oldest fetched covers, as there may be
    /// updated scans on the phantom wiki.
    #[structopt(long)]
    update_old: bool,
}

impl Args {
    pub async fn run(self) -> Result<(), Error> {
        let db = self.db.get_db()?;
        let mut client = WikiClient::new();
        let query = i::issues
            .select((i::id, i::year, i::number_str))
            .left_join(c::covers);
        let query = if self.update_old {
            query.order(c::fetch_time.asc()).limit(10).into_boxed()
        } else {
            query
                .filter(c::image.is_null())
                .order((i::year.desc(), i::number.desc()))
                .into_boxed()
        };
        for (id, year, number_str) in query.load::<(i32, i16, String)>(&db)? {
            if self.no_op {
                println!("Would load cover {:>2}/{}.", number_str, year);
            } else {
                load_cover(&mut client, &db, id, year, &number_str).await?;
            }
        }
        Ok(())
    }
}

async fn load_cover(
    client: &mut WikiClient,
    db: &PgConnection,
    id: i32,
    year: i16,
    number_str: &str,
) -> Result<(), Error> {
    match client.fetchcover(year, number_str).await {
        Ok(imgdata) => {
            diesel::insert_into(c::covers)
                .values((
                    c::issue.eq(id),
                    c::image.eq(imgdata.as_ref()),
                    c::fetch_time.eq(now),
                ))
                .on_conflict(c::issue)
                .do_update()
                .set((
                    c::image.eq(excluded(c::image)),
                    c::fetch_time.eq(excluded(c::fetch_time)),
                ))
                .execute(db)?;
            eprintln!(
                "Got {} bytes of image data for {}/{}",
                imgdata.as_ref().len(),
                number_str,
                year,
            );
        }
        Err(err) => {
            eprintln!(
                "Failed to fetch cover for {}/{}: {}",
                number_str, year, err,
            );
        }
    }
    Ok(())
}

struct WikiClient {
    client: Client,
    sel1: Selector,
    sel2: Selector,
}

impl WikiClient {
    fn new() -> Self {
        WikiClient {
            client: Client::new(),
            sel1: Selector::parse("#bodyContent a.image").unwrap(),
            sel2: Selector::parse(".fullImageLink a").unwrap(),
        }
    }

    async fn fetchcover(
        &mut self,
        year: i16,
        number_str: &str,
    ) -> Result<impl AsRef<[u8]>, Error> {
        let url2 = select_href(
            &self
                .get(&format!("/Fantomen_{}/{}", number_str, year))
                .await?
                .text()
                .await?,
            &self.sel1,
        )?;
        // Scullmark is sometimes used for no cover scanned yet.
        // The Mini_sweden may be the next image when there is no cover image.
        if url2.contains("Scullmark.gif") || url2.contains("Mini_sweden") {
            return Err(format_err!("Cover missing"));
        }
        let imgurl =
            select_href(&self.get(&url2).await?.text().await?, &self.sel2)?;
        Ok(self.get(&imgurl).await?.bytes().await?)
    }

    async fn get(&mut self, url: &str) -> Result<Response, Error> {
        let url1 = format!("http://www.phantomwiki.org{}", url);
        let resp = self.client.get(&url1).send().await?;
        if resp.status().is_success() {
            Ok(resp)
        } else {
            Err(format_err!("Got {}", resp.status()))
        }
    }
}

fn select_href(html: &str, selector: &Selector) -> Result<String, Error> {
    let doc = Html::parse_document(&html);
    let elem = doc
        .select(selector)
        .next()
        .ok_or_else(|| format_err!("Selector {:?} missing", selector))?
        .value();
    let href = elem
        .attr("href")
        .ok_or_else(|| format_err!("Attribute href missing in {:?}", elem))?
        .to_string();
    Ok(href)
}
