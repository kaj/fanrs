use crate::schema::covers::dsl as c;
use crate::schema::issues::dsl as i;
use diesel::dsl::now;
use diesel::pg::PgConnection;
use diesel::prelude::*;
use failure::{format_err, Error};
use reqwest::{Client, Response};
use scraper::{Html, Selector};

pub fn fetch_covers(db: &PgConnection) -> Result<(), Error> {
    let mut client = WikiClient::new();
    for (id, year, number_str) in i::issues
        .left_join(c::covers)
        .filter(c::image.is_null())
        .select((i::id, i::year, i::number_str))
        .order((i::year.desc(), i::number.desc()))
        .load::<(i32, i16, String)>(db)?
    {
        match client.fetchcover(year, &number_str) {
            Ok(imgdata) => {
                diesel::insert_into(c::covers)
                    .values((
                        c::issue.eq(id),
                        c::image.eq(&imgdata),
                        c::fetch_time.eq(now),
                    ))
                    .execute(db)?;
                eprintln!(
                    "Got {} bytes of image data for {}/{}",
                    imgdata.len(),
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

    fn fetchcover(
        &mut self,
        year: i16,
        number_str: &str,
    ) -> Result<Vec<u8>, Error> {
        let url2 = select_href(
            &self
                .get(&format!("/Fantomen_{}/{}", number_str, year))?
                .text()?,
            &self.sel1,
        )?;
        if url2.contains("Scullmark.gif") {
            return Err(format_err!("Cover missing"));
        }
        let imgurl = select_href(&self.get(&url2)?.text()?, &self.sel2)?;
        let mut buf = Vec::new();
        self.get(&imgurl)?.copy_to(&mut buf)?;
        Ok(buf)
    }

    fn get(&mut self, url: &str) -> Result<Response, Error> {
        let url1 = format!("http://www.phantomwiki.org{}", url);
        let resp = self.client.get(&url1).send()?;
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
