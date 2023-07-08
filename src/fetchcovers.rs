use crate::models::IssueRef;
use crate::schema::covers::dsl as c;
use crate::schema::issues::dsl as i;
use crate::DbOpt;
use anyhow::{anyhow, Context, Result};
use diesel::dsl::now;
use diesel::prelude::*;
use diesel::upsert::excluded;
use diesel_async::{AsyncPgConnection, RunQueryDsl};
use reqwest::{self, Client, Response};
use scraper::{Html, Selector};
use std::path::PathBuf;
use tokio::fs::read;

#[derive(clap::Parser)]
pub struct Args {
    #[clap(flatten)]
    db: DbOpt,

    /// No operation, only check which covers would be fetched.
    #[clap(long)]
    no_op: bool,

    /// Update some of the oldest fetched covers, as there may be
    /// updated scans on the phantom wiki.
    #[clap(long)]
    update_old: bool,

    #[clap(subcommand)]
    subcmd: Option<SubCmd>,
}

#[derive(clap::Subcommand)]
enum SubCmd {
    /// Load a cover from a local image file.
    ///
    /// Note: The --no-op option is ignored for this command.
    LoadLocal(LoadLocal),
}

impl Args {
    pub async fn run(self) -> Result<()> {
        let db = self.db.get_db().await?;
        match self.subcmd {
            Some(SubCmd::LoadLocal(local)) => local.run(db).await,
            None => self.do_fetch(db).await,
        }
    }

    async fn do_fetch(self, mut db: AsyncPgConnection) -> Result<()> {
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
        for (id, year, number_str) in
            query.load::<(i32, i16, String)>(&mut db).await?
        {
            if self.no_op {
                println!("Would load cover {number_str:>2}/{year}.");
            } else {
                load_cover(&mut client, &mut db, id, year, &number_str)
                    .await?;
            }
        }
        Ok(())
    }
}

#[derive(clap::Parser)]
struct LoadLocal {
    /// The issue to load a cover for.
    issue: IssueRef,
    /// The file containing the cover.
    /// This should be a jpeg image.
    path: PathBuf,
}

impl LoadLocal {
    async fn run(self, mut db: AsyncPgConnection) -> Result<()> {
        let data = read(self.path).await?;
        println!(
            "Got {} bytes for Fa {}/{}",
            data.len(),
            self.issue.number,
            self.issue.year
        );
        let id =
            self.issue.load_id(&mut db).await.with_context(|| {
                format!("Failed to load {:?}", self.issue)
            })?;
        save_cover(id, &data, &mut db).await?;
        Ok(())
    }
}

async fn load_cover(
    client: &mut WikiClient,
    db: &mut AsyncPgConnection,
    id: i32,
    year: i16,
    number_str: &str,
) -> Result<()> {
    match client.fetchcover(year, number_str).await {
        Ok(imgdata) => {
            save_cover(id, imgdata.as_ref(), db).await?;
            eprintln!(
                "Got {} bytes of image data for {}/{}",
                imgdata.as_ref().len(),
                number_str,
                year,
            );
        }
        Err(err) => {
            eprintln!("Failed to fetch cover for {number_str}/{year}: {err}");
        }
    }
    Ok(())
}

async fn save_cover(
    id: i32,
    imgdata: &[u8],
    db: &mut AsyncPgConnection,
) -> Result<()> {
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
        .execute(db)
        .await?;
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
    ) -> Result<impl AsRef<[u8]>> {
        let url2 = select_href(
            &self
                .get(&format!("/index.php/Fantomen_{number_str}/{year}"))
                .await?
                .text()
                .await?,
            &self.sel1,
        )?;
        // Scullmark is sometimes used for no cover scanned yet.
        // The Mini_sweden may be the next image when there is no cover image.
        if url2.contains("Scullmark.gif") || url2.contains("Mini_sweden") {
            return Err(anyhow!("Cover missing"));
        }
        let imgurl =
            select_href(&self.get(&url2).await?.text().await?, &self.sel2)?;
        Ok(self.get(&imgurl).await?.bytes().await?)
    }

    async fn get(&mut self, url: &str) -> reqwest::Result<Response> {
        let url = format!("https://www.phantomwiki.org{url}");
        self.client.get(&url).send().await?.error_for_status()
    }
}

fn select_href(html: &str, selector: &Selector) -> Result<String> {
    let doc = Html::parse_document(html);
    let elem = doc
        .select(selector)
        .next()
        .ok_or_else(|| anyhow!("Selector {:?} missing", selector))?
        .value();
    let href = elem
        .attr("href")
        .ok_or_else(|| anyhow!("Attribute href missing in {:?}", elem))?
        .to_string();
    Ok(href)
}
