use crate::models::{
    Article, Creator, Episode, Issue, OtherMag, Part, RefKey, Title,
};
use chrono::NaiveDate;
use diesel::prelude::*;
use failure::{format_err, Error};
use io_result_optional::IoResultOptional;
use std::fs::File;
use std::path::Path;
use xmltree::Element;

type Result<T> = std::result::Result<T, Error>;

pub fn load_year(base: &Path, year: i16, db: &PgConnection) -> Result<()> {
    do_load_year(base, year, db)
        .map_err(|e| format_err!("Error reading data for {}: {}", year, e))
}

pub fn do_load_year(base: &Path, year: i16, db: &PgConnection) -> Result<()> {
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
    use crate::schema::episodes::dsl as e;
    use crate::schema::episodes_by::dsl as eb;
    let episode = Episode::get_or_create(
        &Title::get_or_create(get_req_text(&c, "title")?, db)?,
        get_text_norm(c, "episode").as_ref().map(|t| t.as_ref()),
        get_text_norm(c, "teaser").as_ref().map(|t| t.as_ref()),
        get_text_norm(c, "note").as_ref().map(|t| t.as_ref()),
        get_text_norm(c, "copyright").as_ref().map(|t| t.as_ref()),
        db,
    )?;
    let part = c.get_child("part");
    Part::publish(
        &episode,
        part.and_then(|p| p.attributes.get("no"))
            .and_then(|n| n.parse().ok()),
        part.and_then(|p| p.text.as_ref()).map(|s| s.as_ref()),
        &issue,
        Some(seqno as i16),
        get_best_plac(c),
        &get_text_norm(c, "label").unwrap_or_default(),
        db,
    )?;
    for e in &c.children {
        match e.name.as_ref() {
            "episode"
                if e.attributes.get("role") == Some(&"orig".to_string()) =>
            {
                let lang =
                    e.attributes.get("lang").expect("orig should have lang");
                let orig = e
                    .text
                    .as_ref()
                    .map(|t| normalize_space(&t))
                    .expect("orig should have name");
                diesel::update(e::episodes)
                    .set((e::orig_lang.eq(lang), e::orig_episode.eq(orig)))
                    .filter(e::id.eq(episode.id))
                    .execute(db)?;
            }
            "label" | "title" | "episode" | "teaser" | "part" | "note"
            | "copyright" | "best" => (), // handled above
            "by" => {
                let role = e
                    .attributes
                    .get("role")
                    .map(|r| r.as_ref())
                    .unwrap_or("by");
                for by in get_creators(&e, db)? {
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
                    Part::publish(
                        &episode,
                        None,
                        None,
                        &issue,
                        None,
                        get_best_plac(c),
                        &get_text_norm(c, "label").unwrap_or_default(),
                        db,
                    )?;
                }
                Some("date")
                    if e.attributes.get("role")
                        == Some(&"orig".to_string()) =>
                {
                    let date: NaiveDate =
                        get_text(e, "date").unwrap().parse()?;
                    diesel::update(e::episodes)
                        .set((
                            e::orig_date.eq(date),
                            e::orig_to_date.eq(Option::<NaiveDate>::None),
                            e::orig_sundays.eq(false),
                        ))
                        .filter(e::id.eq(episode.id))
                        .execute(db)?;
                }
                Some("magazine") => {
                    let om = OtherMag::get_or_create(
                        get_text_norm(e, "magazine").unwrap(),
                        get_text(e, "issue")
                            .map(|s| s.parse())
                            .transpose()?,
                        get_text(e, "of").map(|s| s.parse()).transpose()?,
                        get_text(e, "year").map(|s| s.parse()).transpose()?,
                        db,
                    )?;
                    diesel::update(e::episodes)
                        .set(e::orig_mag.eq(om.id))
                        .filter(e::id.eq(episode.id))
                        .execute(db)?;
                }
                _other => Err(format_err!("Unknown prevpub {:?}", e))?,
            },
            "daystrip" => {
                if let Some(from) = get_text(e, "from") {
                    let from: NaiveDate = from.parse()?;
                    let to: NaiveDate = get_req_text(e, "to")?.parse()?;
                    let sun =
                        e.attributes.get("d") == Some(&"sun".to_string());
                    diesel::update(e::episodes)
                        .set((
                            e::orig_date.eq(Some(from)),
                            e::orig_to_date.eq(Some(to)),
                            e::orig_sundays.eq(sun),
                        ))
                        .filter(e::id.eq(episode.id))
                        .execute(db)?;
                } else if let Some(from) = get_text(e, "fromnr") {
                    diesel::update(e::episodes)
                        .set((
                            e::strip_from.eq(from.parse::<i32>()?),
                            e::strip_to
                                .eq(get_req_text(e, "tonr")?
                                    .parse::<i32>()?),
                        ))
                        .filter(e::id.eq(episode.id))
                        .execute(db)?;
                } else {
                    Err(format_err!("Unknown daystrip {:?}", e))?;
                }
            }
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
fn get_text_norm<'a>(e: &'a Element, name: &str) -> Option<String> {
    e.get_child(name)
        .and_then(|e| e.text.as_ref().map(|s| normalize_space(&s)))
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

pub fn read_persondata(base: &Path, db: &PgConnection) -> Result<()> {
    use crate::schema::creator_aliases::dsl as ca;
    use crate::schema::creators::dsl as c;
    use slug::slugify;
    let file = File::open(base.join("extra-people.data"))?;
    for e in Element::parse(file)?.children {
        match e.name.as_ref() {
            "person" => {
                let name = get_req_text(&e, "name")?;
                let slug = e
                    .attributes
                    .get("slug")
                    .cloned()
                    .unwrap_or_else(|| slugify(&name));
                let creator = c::creators
                    .select((c::id, c::name, c::slug))
                    .filter(c::name.eq(&name))
                    .filter(c::slug.eq(&slug))
                    .first::<Creator>(db)
                    .optional()?
                    .ok_or(0)
                    .or_else(|_| {
                        diesel::insert_into(c::creators)
                            .values((c::name.eq(&name), c::slug.eq(&slug)))
                            .returning((c::id, c::name, c::slug))
                            .get_result::<Creator>(db)
                            .and_then(|c| {
                                diesel::insert_into(ca::creator_aliases)
                                    .values((
                                        ca::creator_id.eq(c.id),
                                        ca::name.eq(&name),
                                    ))
                                    .execute(db)?;
                                Ok(c)
                            })
                    })?;

                for a in e.children.iter().filter(|a| a.name == "alias") {
                    if let Some(ref alias) = a.text {
                        ca::creator_aliases
                            .select(ca::id)
                            .filter(ca::creator_id.eq(creator.id))
                            .filter(ca::name.eq(&alias))
                            .first::<i32>(db)
                            .optional()?
                            .ok_or(0)
                            .or_else(|_| {
                                diesel::insert_into(ca::creator_aliases)
                                    .values((
                                        ca::creator_id.eq(creator.id),
                                        ca::name.eq(alias),
                                    ))
                                    .returning(ca::id)
                                    .get_result(db)
                            })?;
                    }
                }
            }
            _other => panic!("Unknown element {:?}", e),
        }
    }
    Ok(())
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

fn normalize_space(s: &str) -> String {
    let mut buf = vec![];
    let mut space = true; // ignore leading space
    for ch in s.bytes() {
        if ch.is_ascii_whitespace() {
            if !space {
                buf.push(b' ');
                space = true;
            }
        } else {
            space = false;
            buf.push(ch);
        }
    }
    if space {
        buf.pop();
    }
    String::from_utf8(buf).expect("Normalize space should retain utf8")
}

#[test]
fn test_normalize_space() {
    assert_eq!(normalize_space("foo bar"), "foo bar");
    assert_eq!(normalize_space("\n  foo\n\tbar \n"), "foo bar");
}
