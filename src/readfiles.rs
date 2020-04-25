use crate::models::{
    Article, Creator, Episode, Issue, OtherMag, Part, RefKey, Title,
};
use crate::DbOpt;
use chrono::{Datelike, Local, NaiveDate};
use diesel::prelude::*;
use diesel::sql_query;
use failure::{format_err, Error};
use roxmltree::{Document, Node};
use slug::slugify;
use std::fs::read_to_string;
use std::path::{Path, PathBuf};
use std::time::Instant;
use structopt::StructOpt;

type Result<T> = std::result::Result<T, Error>;
static XMLNS: &str = "http://www.w3.org/XML/1998/namespace";

#[derive(StructOpt)]
pub struct Args {
    #[structopt(flatten)]
    db: DbOpt,

    /// The directory containing the data files.
    #[structopt(long, short, parse(from_os_str), env = "FANTOMEN_DATA")]
    basedir: PathBuf,

    /// Read data for all years, from 1950 to current.
    #[structopt(long, short)]
    all: bool,

    /// Year(s) to read data for.
    #[structopt(name = "year")]
    years: Vec<u32>,
}

impl Args {
    pub fn run(self) -> Result<()> {
        let db = self.db.get_db()?;
        read_persondata(&self.basedir, &db)?;
        if self.all {
            let current_year = Local::now().year() as i16;
            for year in 1950..=current_year {
                load_year(&self.basedir, year, &db)?;
            }
        } else {
            if self.years.is_empty() {
                return Err(format_err!(
                    "No year(s) to read files for given."
                ));
            }
            for year in self.years {
                load_year(&self.basedir, year as i16, &db)?;
            }
        }
        delete_unpublished(&db)?;
        let start = Instant::now();
        sql_query("refresh materialized view creator_contributions;")
            .execute(&db)?;
        println!("Updated creators view in {:?}", start.elapsed());
        Ok(())
    }
}

fn load_year(base: &Path, year: i16, db: &PgConnection) -> Result<()> {
    do_load_year(base, year, db)
        .map_err(|e| format_err!("Error reading data for {}: {}", year, e))
}

fn do_load_year(base: &Path, year: i16, db: &PgConnection) -> Result<()> {
    match read_to_string(base.join(format!("{}.data", year))) {
        Ok(data) => {
            for elem in child_elems(Document::parse(&data)?.root_element()) {
                match elem.tag_name().name() {
                    "info" => (), // ignore
                    "issue" => {
                        register_issue(year, elem, db).map_err(|e| {
                            format_err!(
                                "In issue {}: {}",
                                elem.attribute("nr").unwrap_or("?"),
                                e
                            )
                        })?
                    }
                    _other => {
                        return Err(format_err!("Unexpected {:?}", elem));
                    }
                }
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("No data found for {}", year);
        }
        Err(e) => {
            return Err(e.into());
        }
    }
    Ok(())
}

fn register_issue(year: i16, i: Node, db: &PgConnection) -> Result<()> {
    let nr = i
        .attribute("nr")
        .ok_or_else(|| format_err!("nr missing"))
        .and_then(|s| Ok(s.parse()?))?;
    let issue = Issue::get_or_create(
        year,
        nr,
        i.attribute("pages").and_then(|s| s.parse().ok()),
        i.attribute("price").and_then(|s| s.parse().ok()),
        get_child(i, "omslag").and_then(get_best_plac),
        db,
    )?;
    println!("Found issue {}", issue);
    issue.clear(db)?;

    for (seqno, c) in child_elems(i).enumerate() {
        match c.tag_name().name() {
            "omslag" => {
                if let Some(by) = get_child(c, "by") {
                    for creator in get_creators(by, db)? {
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
            "text" => register_article(&issue, seqno, c, db)?,
            "serie" => register_serie(&issue, seqno, c, db)?,
            "skick" => (), // ignore
            _ => {
                return Err(format_err!(
                    "Unexpected element {:?} in issue {}",
                    c,
                    issue,
                ))
            }
        }
    }
    Ok(())
}

fn register_article(
    issue: &Issue,
    seqno: usize,
    c: Node,
    db: &PgConnection,
) -> Result<()> {
    let article = Article::get_or_create(
        get_req_text(c, "title")?,
        get_text_norm(c, "subtitle").as_deref(),
        get_text_norm(c, "note").as_deref(),
        db,
    )?;
    article.publish(issue.id, seqno as i16, db)?;
    for e in child_elems(c) {
        match e.tag_name().name() {
            "title" | "subtitle" | "note" => (), // handled above
            "by" => {
                let role = e.attribute("role").unwrap_or("by");
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
            "ref" => article.set_refs(&parse_refs(e)?, db)?,
            _other => return Err(format_err!("Unknown {:?} in text", e)),
        }
    }
    Ok(())
}

fn register_serie(
    issue: &Issue,
    seqno: usize,
    c: Node,
    db: &PgConnection,
) -> Result<()> {
    use crate::schema::episodes::dsl as e;
    use crate::schema::episodes_by::dsl as eb;
    let episode = Episode::get_or_create(
        &Title::get_or_create(get_req_text(c, "title")?, db)?,
        get_text_norm(c, "episode").as_deref(),
        get_text_norm(c, "teaser").as_deref(),
        get_text_norm(c, "note").as_deref(),
        get_text_norm(c, "copyright").as_deref(),
        db,
    )?;
    let part = get_child(c, "part");
    Part::publish(
        &episode,
        part.and_then(|p| p.attribute("no"))
            .and_then(|n| n.parse().ok()),
        part.and_then(|p| p.text()),
        &issue,
        Some(seqno as i16),
        get_best_plac(c),
        &get_text_norm(c, "label").unwrap_or_default(),
        db,
    )?;
    for e in child_elems(c) {
        match e.tag_name().name() {
            "episode" if e.attribute("role") == Some("orig") => {
                let lang = e
                    .attribute((XMLNS, "lang"))
                    .expect("orig should have lang");
                let orig = e
                    .text()
                    .map(normalize_space)
                    .expect("orig should have name");
                diesel::update(e::episodes)
                    .set((e::orig_lang.eq(lang), e::orig_episode.eq(orig)))
                    .filter(e::id.eq(episode.id))
                    .execute(db)?;
            }
            "label" | "title" | "episode" | "teaser" | "part" | "note"
            | "copyright" | "best" => (), // handled above
            "by" => {
                let role = e.attribute("role").unwrap_or("by");
                for by in get_creators(e, db)? {
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
            "ref" => {
                episode.set_refs(&parse_refs(e)?, db).map_err(|err| {
                    format_err!("{} while handling {:?}", err, e)
                })?
            }
            "prevpub" => match e
                .first_element_child()
                .map(|e| e.tag_name().name())
            {
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
                Some("date") if e.attribute("role") == Some("orig") => {
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
                _other => return Err(format_err!("Unknown prevpub {:?}", e)),
            },
            "daystrip" => {
                if let Some(from) = get_text(e, "from") {
                    let from: NaiveDate = from.parse()?;
                    let to: NaiveDate = get_req_text(e, "to")?.parse()?;
                    let sun = e.attribute("d") == Some("sun");
                    diesel::update(e::episodes)
                        .set((
                            e::orig_date.eq(Some(from)),
                            e::orig_to_date.eq(Some(to)),
                            e::orig_sundays.eq(sun),
                        ))
                        .filter(e::id.eq(episode.id))
                        .execute(db)?;
                } else if let Some(from) = get_text(e, "fromnr") {
                    let to = get_req_text(e, "tonr")?;
                    diesel::update(e::episodes)
                        .set((
                            e::strip_from.eq(from.parse::<i32>()?),
                            e::strip_to.eq(to.parse::<i32>()?),
                        ))
                        .filter(e::id.eq(episode.id))
                        .execute(db)?;
                } else {
                    return Err(format_err!("Unknown daystrip {:?}", e));
                }
            }
            _other => return Err(format_err!("Unknown {:?} in serie", e)),
        }
    }
    Ok(())
}

fn parse_refs(parent: Node) -> Result<Vec<RefKey>> {
    child_elems(parent).map(parse_ref).collect()
}

fn parse_ref(e: Node) -> Result<RefKey> {
    match e.tag_name().name() {
        "fa" => e.attribute("no").map(RefKey::fa).ok_or("Fa without no"),
        "key" => e.text().map(RefKey::key).ok_or("Key without text"),
        "who" => e.text().map(RefKey::who).ok_or("Who without name"),
        "serie" => e.text().map(RefKey::title).ok_or("Serie without title"),
        _ => Err("Unknown reference"),
    }
    .map_err(|err| format_err!("{} in {:?}", err, e))
}

#[test]
fn test_parse_refs() -> Result<()> {
    let doc = "<ref>
        <fa no=\"12\"/>
        <key>Marion Trelawny</key>
        <who>Jaime Vallvé</who>
        <serie>Johan Vilde</serie>
      </ref>\n";

    assert_eq!(
        parse_refs(Document::parse(doc)?.root_element())?,
        vec![
            RefKey::fa("12"),
            RefKey::key("Marion Trelawny"),
            RefKey::who("Jaime Vallvé"),
            RefKey::title("Johan Vilde"),
        ],
    );
    Ok(())
}

fn get_req_text<'a>(e: Node<'a, 'a>, name: &str) -> Result<&'a str> {
    get_text(e, name)
        .ok_or_else(|| format_err!("{:?} missing child {}", e, name))
}

fn get_text<'a>(e: Node<'a, 'a>, name: &str) -> Option<&'a str> {
    get_child(e, name).and_then(|e| e.text())
}

fn get_text_norm(e: Node, name: &str) -> Option<String> {
    get_text(e, name).map(normalize_space)
}

fn get_best_plac(e: Node) -> Option<i16> {
    get_child(e, "best")
        .and_then(|e| e.attribute("plac").and_then(|s| s.parse().ok()))
}

fn get_creators(by: Node, db: &PgConnection) -> Result<Vec<Creator>> {
    let one_creator = |e: Node| -> Result<Creator> {
        let name = e
            .text()
            .ok_or_else(|| format_err!("missing name in {:?}", e))?;
        Ok(Creator::get_or_create(name, db).map_err(|e| {
            format_err!("Failed to create creator {:?}: {}", name, e)
        })?)
    };
    let who = child_elems(by)
        .map(one_creator)
        .collect::<Result<Vec<_>>>()?;
    if who.is_empty() {
        // No child elements, a single name directly in the by element
        Ok(vec![one_creator(by)?])
    } else {
        // Child <who> elements, each containing a name.
        Ok(who)
    }
}

fn read_persondata(base: &Path, db: &PgConnection) -> Result<()> {
    use crate::schema::creator_aliases::dsl as ca;
    use crate::schema::creators::dsl as c;
    let buf = read_to_string(base.join("extra-people.data"))?;
    for e in child_elems(Document::parse(&buf)?.root_element()) {
        match e.tag_name().name() {
            "person" => {
                let name = get_req_text(e, "name")?;
                let slug = e
                    .attribute("slug")
                    .map(String::from)
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

                for a in
                    e.children().filter(|a| a.tag_name().name() == "alias")
                {
                    if let Some(alias) = a.text() {
                        ca::creator_aliases
                            .select(ca::id)
                            .filter(ca::creator_id.eq(creator.id))
                            .filter(ca::name.eq(alias))
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

fn delete_unpublished(db: &PgConnection) -> Result<()> {
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

fn get_child<'a>(node: Node<'a, 'a>, name: &str) -> Option<Node<'a, 'a>> {
    node.children().find(|a| a.tag_name().name() == name)
}

fn child_elems<'a, 'b>(
    node: Node<'a, 'b>,
) -> impl Iterator<Item = Node<'a, 'b>> {
    node.children().filter(|n| n.is_element())
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
