use crate::models::{Article, Creator, Episode, Issue, Part, RefKey, Title};
use diesel::prelude::*;
use failure::{format_err, Error, Fail};
use std::fs::File;
use xmltree::Element;

pub fn load_year(year: i16, db: &PgConnection) -> Result<(), Error> {
    let data = File::open(format!("/home/kaj/proj/fantomen/{}.data", year))
        .map_err(|e| fail_year(year, &e))
        .and_then(|f| Element::parse(f).map_err(|e| fail_year(year, &e)))?;

    for i in data.children {
        if i.name == "info" {
            // ignore
        } else if i.name == "issue" {
            let (nr, nr_str) = i
                .attributes
                .get("nr")
                .ok_or_else(|| format_err!("nr missing"))
                .and_then(|s| parse_nr(s))?;
            let issue = Issue::get_or_create(
                year,
                nr,
                nr_str,
                i.attributes.get("pages").and_then(|s| s.parse().ok()),
                i.attributes.get("price").and_then(|s| s.parse().ok()),
                db,
            )?;
            println!("Found issue {}", issue);
            issue.clear(db)?;

            for (seqno, c) in i.children.iter().enumerate() {
                if c.name == "omslag" {
                    if let Some(by) = c.get_child("by") {
                        let by = get_creators(by, db)?;
                        for creator in by {
                            use crate::schema::cover_by::dsl as cb;
                            diesel::insert_into(cb::cover_by)
                                .values((
                                    cb::issue_id.eq(issue.id),
                                    cb::by_id.eq(creator.id),
                                ))
                                .on_conflict_do_nothing()
                                .execute(db)?;
                        }
                    }
                    let best = get_best_plac(c);
                    println!("  -> omslag {:?}", best);
                } else if c.name == "text" {
                    let article = Article::get_or_create(
                        get_req_text(&c, "title")?,
                        get_text(&c, "subtitle"),
                        get_text(&c, "note"),
                        db,
                    )?;
                    if let Some(ref refs) = get_refs(c)? {
                        article.set_refs(&refs, db)?;
                    }
                    article.publish(issue.id, Some(seqno as i16), db)?;
                    for by in c.children.iter().filter(|e| e.name == "by") {
                        let role = by
                            .attributes
                            .get("role")
                            .map(|r| r.as_ref())
                            .unwrap_or("by");
                        for by in get_creators(by, db)? {
                            use crate::schema::articles_by::dsl as ab;
                            diesel::insert_into(ab::articles_by)
                                .values((
                                    ab::article_id.eq(article.id),
                                    ab::by_id.eq(by.id),
                                    ab::role.eq(role),
                                ))
                                .on_conflict_do_nothing()
                                .execute(db)?;
                        }
                    }
                } else if c.name == "serie" {
                    let title =
                        Title::get_or_create(get_req_text(&c, "title")?, db)?;
                    let episode = Episode::get_or_create(
                        &title,
                        get_text(&c, "episode"),
                        get_text(&c, "teaser"),
                        get_text(&c, "note"),
                        get_text(&c, "copyright"),
                        db,
                    )?;
                    let part = Part::of(&c);
                    episode.publish_part(
                        part.as_ref(),
                        issue.id,
                        Some(seqno as i16),
                        get_best_plac(c),
                        db,
                    )?;
                    if let Some(ref refs) = get_refs(c)? {
                        episode.set_refs(&refs, db)?;
                    }
                    for by in c.children.iter().filter(|e| e.name == "by") {
                        let role = by
                            .attributes
                            .get("role")
                            .map(|r| r.as_ref())
                            .unwrap_or("by");
                        for by in get_creators(by, db)? {
                            use crate::schema::creativeparts::dsl as cp;
                            diesel::insert_into(cp::creativeparts)
                                .values((
                                    cp::episode_id.eq(episode.id),
                                    cp::by_id.eq(by.id),
                                    cp::role.eq(role),
                                ))
                                .on_conflict_do_nothing()
                                .execute(db)?;
                        }
                    }
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

fn fail_year(year: i16, err: &Fail) -> Error {
    format_err!("Failed to read data for {}: {}", year, err)
}

fn parse_nr(nr_str: &str) -> Result<(i16, &str), Error> {
    let nr = nr_str
        .find('-')
        .map(|p| &nr_str[0..p])
        .unwrap_or(nr_str)
        .parse()
        .map_err(|e| format_err!("Bad nr: {:?} {}", nr_str, e))?;
    Ok((nr, nr_str))
}

fn get_refs(e: &Element) -> Result<Option<Vec<RefKey>>, Error> {
    if let Some(ref refs) = e.get_child("ref") {
        let refs = refs
            .children
            .iter()
            .map(parse_ref)
            .collect::<Result<Vec<RefKey>, _>>()?;
        Ok(Some(refs))
    } else {
        Ok(None)
    }
}

fn parse_ref(e: &Element) -> Result<RefKey, Error> {
    match e.name.as_ref() {
        "fa" => e
            .attributes
            .get("no")
            .map(|s| RefKey::fa(s))
            .ok_or_else(|| format_err!("Fa witout no: {:?}", e)),
        "key" => e
            .text
            .as_ref()
            .map(|s| RefKey::key(s))
            .ok_or_else(|| format_err!("Key without text: {:?}", e)),
        "who" => e
            .text
            .as_ref()
            .map(|s| RefKey::who(s))
            .ok_or_else(|| format_err!("Who without name: {:?}", e)),
        "serie" => e
            .text
            .as_ref()
            .map(|s| RefKey::title(s))
            .ok_or_else(|| format_err!("Serie without title: {:?}", e)),
        _ => Err(format_err!("Unknown refernce: {:?}", e)),
    }
}

fn get_req_text<'a>(e: &'a Element, name: &str) -> Result<&'a str, Error> {
    get_text(e, name)
        .ok_or_else(|| format_err!("{:?} missing child {}", e, name))
}

fn get_text<'a>(e: &'a Element, name: &str) -> Option<&'a str> {
    e.get_child(name)
        .and_then(|e| e.text.as_ref().map(|s| s.as_ref()))
}

fn get_best_plac(e: &Element) -> Option<i16> {
    e.get_child("best")
        .and_then(|e| e.attributes.get("plac").and_then(|s| s.parse().ok()))
}

fn get_creators(
    by: &Element,
    db: &PgConnection,
) -> Result<Vec<Creator>, Error> {
    let one_creator = |e: &Element| {
        let name = &e
            .text
            .as_ref()
            .ok_or_else(|| format_err!("missing name in {:?}", e))?;
        Ok(Creator::get_or_create(name, db).map_err(|e| {
            format_err!("Failed to create creator {:?}: {}", name, e)
        })?)
    };
    let c = &by.children;
    if c.is_empty() {
        // No child elements, a single name directly in the by element
        Ok(vec![one_creator(by)?])
    } else {
        // Child <who> elements, each containing a name.
        c.iter().map(one_creator).collect()
    }
}
