use bigdecimal::BigDecimal;
use diesel::pg::PgConnection;
use failure::Error;
use models::{Episode, Issue, Part, Title};
use std::fs::File;
use xmltree::Element;

pub fn load_year(year: i16, db: &PgConnection) -> Result<(), Error> {
    let file = File::open(format!("/home/kaj/proj/fantomen/{}.data", year))?;
    let data = Element::parse(file)?;

    for i in data.children {
        if i.name == "info" {
            // ignore
        } else if i.name == "issue" {
            let nr_str = i
                .attributes
                .get("nr")
                .ok_or_else(|| format_err!("nr missing"))?;
            let nr = nr_str.parse::<i16>()?;
            let pages = i
                .attributes
                .get("pages")
                .and_then(|s| s.parse::<i16>().ok());
            let price = i
                .attributes
                .get("price")
                .and_then(|s| s.parse::<BigDecimal>().ok());
            let issue =
                Issue::get_or_create(year, nr, nr_str, pages, price, db)?;
            println!("Found issue {}", issue);
            for c in i.children {
                if c.name == "omslag" {
                    let by = c.get_child("by").and_then(|e| e.text.as_ref());
                    let best = c.get_child("best").and_then(|e| {
                        e.attributes
                            .get("plac")
                            .and_then(|s| s.parse::<i16>().ok())
                    });
                    println!("  -> omslag {:?} {:?}", by, best);
                } else if c.name == "text" {
                    let title = get_text(&c, "title");
                    let subtitle = get_text(&c, "subtitle");
                    println!("  -> text {:?} {:?}", title, subtitle);
                } else if c.name == "serie" {
                    let title = Title::get_or_create(
                        get_text(&c, "title")
                            .ok_or(format_err!("title missing"))?,
                        db,
                    )?;
                    let episode = get_text(&c, "episode");
                    let teaser = get_text(&c, "teaser");
                    let note = get_text(&c, "note");
                    let copyright = get_text(&c, "copyright");
                    let episode = Episode::get(&title, episode, db)?
                        .map(|episode| {
                            episode.set_details(teaser, note, copyright, db)
                        })
                        .unwrap_or_else(|| {
                            Episode::create(
                                &title, episode, teaser, note, copyright, db,
                            )
                        })?;
                    let part = Part::of(&c);
                    episode.publish_part(
                        part.as_ref(),
                        issue.id,
                        None,
                        db,
                    )?;
                } else if c.name == "skick" {
                    // ignore
                } else {
                    return Err(format_err!("Unexepcetd element: {:?}", c));
                }
            }
        }
        //println!("{:?}", c);
    }
    Ok(())
}

fn get_text<'a>(e: &'a Element, name: &str) -> Option<&'a str> {
    e.get_child(name)
        .and_then(|e| e.text.as_ref().map(|s| s.as_ref()))
}
