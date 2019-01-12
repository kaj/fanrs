use crate::models::{Article, Creator, Episode, Issue, Part, RefKey, Title};
use diesel::prelude::*;
use failure::{format_err, Error};
use io_result_optional::IoResultOptional;
use std::fs::File;
use std::path::Path;
use xmltree::Element;

type Result<T> = std::result::Result<T, Error>;

pub fn load_year(year: i16, db: &PgConnection) -> Result<()> {
    do_load_year(year, db)
        .map_err(|e| format_err!("Error reading data for {}: {}", year, e))
}

pub fn do_load_year(year: i16, db: &PgConnection) -> Result<()> {
    let base = Path::new("/home/kaj/proj/fantomen");
    if let Some(file) =
        File::open(base.join(format!("{}.data", year))).optional()?
    {
        for i in Element::parse(file)?.children {
            match i.name.as_ref() {
                "info" => (), // ignore
                "issue" => register_issue(year, &i, db)?,
                other => Err(format_err!(
                    "Unexpected element {:?} in year {}",
                    other,
                    year,
                ))?,
            }
        }
    } else {
        eprintln!("No data found for {}", year);
    };
    Ok(())
}

fn register_issue(year: i16, i: &Element, db: &PgConnection) -> Result<()> {
    let nr = i
        .attributes
        .get("nr")
        .ok_or_else(|| format_err!("nr missing"))
        .and_then(|s| Ok(s.parse()?))?;
    let issue = Issue::get_or_create(
        year,
        nr,
        i.attributes.get("pages").and_then(|s| s.parse().ok()),
        i.attributes.get("price").and_then(|s| s.parse().ok()),
        i.get_child("omslag").and_then(get_best_plac),
        db,
    )?;
    println!("Found issue {}", issue);
    issue.clear(db)?;

    for (seqno, c) in i.children.iter().enumerate() {
        match c.name.as_ref() {
            "omslag" => {
                if let Some(by) = c.get_child("by") {
                    let by = get_creators(by, db)?;
                    for creator in by {
                        use crate::schema::covers_by::dsl as cb;
                        diesel::insert_into(cb::covers_by)
                            .values((
                                cb::issue_id.eq(issue.id),
                                cb::by_id.eq(creator.id),
                            ))
                            .on_conflict_do_nothing()
                            .execute(db)?;
                    }
                }
            }
            "text" => register_article(&issue, seqno, &c, db)?,
            "serie" => register_serie(&issue, seqno, &c, db)?,
            "skick" => (), // ignore
            _ => Err(format_err!(
                "Unexpected element {:?} in issue {}",
                c,
                issue,
            ))?,
        }
    }
    Ok(())
}

fn register_article(
    issue: &Issue,
    seqno: usize,
    c: &Element,
    db: &PgConnection,
) -> Result<()> {
    let article = Article::get_or_create(
        get_req_text(c, "title")?,
        get_text(c, "subtitle"),
        get_text(c, "note"),
        db,
    )?;
    article.publish(issue.id, seqno as i16, db)?;
    for e in &c.children {
        match e.name.as_ref() {
            "title" | "subtitle" | "note" => (), // handled above
            "by" => {
                let role = e
                    .attributes
                    .get("role")
                    .map(|r| r.as_ref())
                    .unwrap_or("by");
                for by in get_creators(e, db)? {
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
            "ref" => article.set_refs(&parse_refs(&e.children)?, db)?,
            _other => Err(format_err!("Unknown {:?} in text", e))?,
        }
    }
    Ok(())
}

fn register_serie(
    issue: &Issue,
    seqno: usize,
    c: &Element,
    db: &PgConnection,
) -> Result<()> {
    let episode = Episode::get_or_create(
        &Title::get_or_create(get_req_text(&c, "title")?, db)?,
        get_text(&c, "episode"),
        get_text(&c, "teaser"),
        get_text(&c, "note"),
        get_text(&c, "copyright"),
        db,
    )?;
    episode.publish_part(
        Part::of(&c).as_ref(),
        issue.id,
        Some(seqno as i16),
        get_best_plac(c),
        db,
    )?;
    for e in &c.children {
        match e.name.as_ref() {
            "title" | "episode" | "teaser" | "part" | "note"
            | "copyright" | "best" => (), // handled above
            "by" => {
                let role = e
                    .attributes
                    .get("role")
                    .map(|r| r.as_ref())
                    .unwrap_or("by");
                for by in get_creators(&e, db)? {
                    use crate::schema::episodes_by::dsl as eb;
                    diesel::insert_into(eb::episodes_by)
                        .values((
                            eb::episode_id.eq(episode.id),
                            eb::by_id.eq(by.id),
                            eb::role.eq(role),
                        ))
                        .on_conflict_do_nothing()
                        .execute(db)?;
                }
            }
            "ref" => episode.set_refs(&parse_refs(&e.children)?, db)?,
            "prevpub" => match e.children.get(0).map(|e| e.name.as_ref()) {
                Some("fa") => {
                    let nr = get_text(e, "fa").unwrap().parse()?;
                    let year = get_text(e, "year")
                        .ok_or_else(|| format_err!("year missing"))?
                        .parse()?;
                    let issue =
                        Issue::get_or_create(year, nr, None, None, None, db)?;
                    episode.publish_part(
                        None,
                        issue.id,
                        None,
                        get_best_plac(c),
                        db,
                    )?;
                }
                Some("date") => eprintln!("Got prevpub date {:?}", e),
                Some("magazine") => eprintln!("Got magazine date {:?}", e),
                _other => Err(format_err!("Unknown prevpub {:?}", e))?,
            },
            "label" => (),    // TODO
            "daystrip" => (), // TODO
            _other => Err(format_err!("Unknown {:?} in serie", e))?,
        }
    }
    Ok(())
}

fn parse_refs(refs: &[Element]) -> Result<Vec<RefKey>> {
    refs.iter().map(parse_ref).collect()
}

fn parse_ref(e: &Element) -> Result<RefKey> {
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

fn get_req_text<'a>(e: &'a Element, name: &str) -> Result<&'a str> {
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

fn get_creators(by: &Element, db: &PgConnection) -> Result<Vec<Creator>> {
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

pub fn delete_unpublished(db: &PgConnection) -> Result<()> {
    use crate::schema::article_refkeys::dsl as ar;
    use crate::schema::articles::dsl as a;
    use crate::schema::articles_by::dsl as ab;
    use crate::schema::episode_parts::dsl as ep;
    use crate::schema::episode_refkeys::dsl as er;
    use crate::schema::episodes::dsl as e;
    use crate::schema::episodes_by::dsl as eb;
    use crate::schema::publications::dsl as p;
    use crate::schema::refkeys::dsl as r;
    use crate::schema::titles::dsl as t;
    use diesel::dsl::{all, any};

    // Note: Loading these is an inefficiency, but it is the only way I find
    // to get rid of the nullability of publications(episode_part) before
    // comparing to non-nullable episode_parts(id).
    let published_parts = p::publications
        .filter(p::episode_part.is_not_null())
        .select(p::episode_part)
        .distinct()
        .load(db)?
        .into_iter()
        .filter_map(|e| e)
        .collect::<Vec<i32>>();
    let n = diesel::delete(
        ep::episode_parts.filter(ep::id.ne(all(published_parts))),
    )
    .execute(db)?;
    println!("Delete {} junk episode parts.", n);

    let n = diesel::delete(er::episode_refkeys.filter(er::episode_id.eq(
        any(e::episodes.select(e::id).filter(
            e::id.ne(all(ep::episode_parts.select(ep::episode).distinct())),
        )),
    )))
    .execute(db)?;
    println!("Delete {} junk episode refkeys.", n);

    let n = diesel::delete(eb::episodes_by.filter(eb::episode_id.eq(any(
        e::episodes.select(e::id).filter(
            e::id.ne(all(ep::episode_parts.select(ep::episode).distinct())),
        ),
    ))))
    .execute(db)?;
    println!("Delete {} junk episodes-by.", n);

    let n = diesel::delete(e::episodes.filter(
        e::id.ne(all(ep::episode_parts.select(ep::episode).distinct())),
    ))
    .execute(db)?;
    println!("Delete {} junk episodes.", n);

    let n = diesel::delete(
        t::titles
            .filter(t::id.ne(all(e::episodes.select(e::title).distinct()))),
    )
    .execute(db)?;
    println!("Delete {} junk titles.", n);

    let published_articles = p::publications
        .filter(p::article_id.is_not_null())
        .select(p::article_id)
        .distinct()
        .load(db)?
        .into_iter()
        .filter_map(|e| e)
        .collect::<Vec<i32>>();

    let n = diesel::delete(
        ar::article_refkeys
            .filter(ar::article_id.ne(all(&published_articles))),
    )
    .execute(db)?;
    println!("Delete {} junk article refkeys.", n);

    let n = diesel::delete(
        r::refkeys
            .filter(r::id.ne(all(er::episode_refkeys.select(er::refkey_id))))
            .filter(r::id.ne(all(ar::article_refkeys.select(ar::refkey_id)))),
    )
    .execute(db)?;
    println!("Delete {} junk refkeys.", n);

    let n = diesel::delete(
        ab::articles_by.filter(ab::article_id.ne(all(&published_articles))),
    )
    .execute(db)?;
    println!("Delete {} junk articles-by.", n);

    let n = diesel::delete(
        a::articles.filter(a::id.ne(all(&published_articles))),
    )
    .execute(db)?;
    println!(
        "Delete {} junk articles ({} remains).",
        n,
        published_articles.len()
    );
    Ok(())
}
