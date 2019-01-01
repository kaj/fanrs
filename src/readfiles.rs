use crate::models::{
    parse_nr, Article, Creator, Episode, Issue, Part, RefKey, Title,
};
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
                .and_then(|s| Ok(parse_nr(s)?))?;
            let issue = Issue::get_or_create(
                year,
                nr,
                nr_str,
                i.attributes.get("pages").and_then(|s| s.parse().ok()),
                i.attributes.get("price").and_then(|s| s.parse().ok()),
                i.get_child("omslag").and_then(get_best_plac),
                db,
            )?;
            println!("Found issue {}", issue);
            issue.clear(db)?;

            for (seqno, c) in i.children.iter().enumerate() {
                if c.name == "omslag" {
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
                    article.publish(issue.id, seqno as i16, db)?;
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
                    register_published_content(&issue, seqno, &c, db)?;
                } else if c.name == "skick" {
                    // ignore
                } else {
                    return Err(format_err!("Unexepcetd element: {:?}", c));
                }
            }
        }
    }
    Ok(())
}

fn fail_year(year: i16, err: &Fail) -> Error {
    format_err!("Failed to read data for {}: {}", year, err)
}

fn register_published_content(
    issue: &Issue,
    seqno: usize,
    c: &Element,
    db: &PgConnection,
) -> Result<(), Error> {
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
    Ok(())
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

pub fn delete_unpublished(db: &PgConnection) -> Result<(), Error> {
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
