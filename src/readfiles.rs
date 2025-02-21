use crate::DbOpt;
use crate::models::{
    Article, Creator, Episode, Issue, OtherMag, Part, RefKey, Title,
};
use anyhow::{Context, Result, anyhow, bail};
use chrono::{Datelike, Local, NaiveDate};
use diesel::associations::HasTable;
use diesel::pg::Pg;
use diesel::prelude::*;
use diesel::query_builder::{IntoUpdateTarget, QueryFragment, QueryId};
use diesel::sql_query;
use diesel_async::{AsyncPgConnection, RunQueryDsl};
use roxmltree::{Document, Node};
use slug::slugify;
use std::fs::read_to_string;
use std::path::{Path, PathBuf};
use std::time::Instant;

static XMLNS: &str = "http://www.w3.org/XML/1998/namespace";

#[derive(clap::Parser)]
pub struct Args {
    #[clap(flatten)]
    db: DbOpt,

    /// The directory containing the data files.
    #[arg(long, short, env = "FANTOMEN_DATA")]
    basedir: PathBuf,

    /// Read data for all years, from 1950 to current.
    #[arg(long, short)]
    all: bool,

    /// Year(s) to read data for.
    #[arg(name = "year")]
    years: Vec<i16>,
}

impl Args {
    pub async fn run(self) -> Result<()> {
        if self.years.is_empty() && !self.all {
            bail!("No year specified for reading.");
        }
        let mut db = self.db.get_db().await?;
        read_persondata(&self.basedir, &mut db).await?;
        if self.all {
            let current_year = i16::try_from(Local::now().year())?;
            for year in 1950..=current_year {
                load_year(&self.basedir, year, &mut db).await?;
            }
        } else {
            for year in self.years {
                load_year(&self.basedir, year, &mut db).await?;
            }
        }
        delete_unpublished(&mut db).await?;
        let start = Instant::now();
        sql_query("refresh materialized view creator_contributions;")
            .execute(&mut db)
            .await?;
        println!("Updated creators view in {:.3?}", start.elapsed());
        Ok(())
    }
}

async fn load_year(
    base: &Path,
    year: i16,
    db: &mut AsyncPgConnection,
) -> Result<()> {
    do_load_year(base, year, db)
        .await
        .with_context(|| format!("Failed to read data for {year}"))
}

async fn do_load_year(
    base: &Path,
    year: i16,
    db: &mut AsyncPgConnection,
) -> Result<()> {
    match read_to_string(base.join(format!("{year}.data"))) {
        Ok(data) => {
            for elem in child_elems(Document::parse(&data)?.root_element()) {
                match elem.tag_name().name() {
                    "info" => (), // ignore
                    "issue" => {
                        register_issue(year, elem, db).await.with_context(
                            || {
                                format!(
                                    "Error reading issue {}:",
                                    elem.attribute("nr").unwrap_or("?"),
                                )
                            },
                        )?;
                    }
                    _ => return Err(unexpected_element(&elem)),
                }
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("No data found for {year}");
        }
        Err(e) => {
            return Err(e.into());
        }
    }
    Ok(())
}

async fn register_issue<'a>(
    year: i16,
    i: Node<'a, 'a>,
    db: &mut AsyncPgConnection,
) -> Result<()> {
    let nr =
        parse_attribute(i, "nr")?.ok_or_else(|| anyhow!("nr missing"))?;
    let issue = Issue::get_or_create(
        year,
        nr,
        parse_attribute(i, "pages")?,
        parse_attribute(i, "price")?,
        get_child(i, "omslag")
            .and_then(|e| get_best_plac(e).transpose())
            .transpose()?,
        db,
    )
    .await
    .context("issue")?;
    println!("Found issue {issue}");
    issue.clear(db).await?;

    for (c, seqno) in child_elems(i).zip(0i16..) {
        match c.tag_name().name() {
            "omslag" => {
                if let Some(by) = get_child(c, "by") {
                    use crate::schema::covers_by::dsl as cb;
                    let creators = get_creators(by, db)
                        .await?
                        .into_iter()
                        .map(|c| c.id)
                        .collect::<Vec<_>>();
                    diesel::insert_into(cb::covers_by)
                        .values(
                            &creators
                                .iter()
                                .map(|c| {
                                    (
                                        cb::issue_id.eq(issue.id),
                                        cb::creator_alias_id.eq(c),
                                    )
                                })
                                .collect::<Vec<_>>(),
                        )
                        .on_conflict_do_nothing()
                        .execute(db)
                        .await?;
                    let purged = diesel::delete(cb::covers_by)
                        .filter(cb::issue_id.eq(issue.id))
                        .filter(cb::creator_alias_id.ne_all(creators))
                        .execute(db)
                        .await?;
                    if purged > 0 {
                        println!("Removed {purged} bogus cover artists.");
                    }
                }
            }
            "text" => register_article(&issue, seqno, c, db)
                .await
                .context("text")?,
            "serie" => register_serie(&issue, seqno, c, db)
                .await
                .context("serie")?,
            "skick" => (), // ignore
            _ => return Err(unexpected_element(&c)),
        }
    }
    Ok(())
}

async fn register_article<'a>(
    issue: &Issue,
    seqno: i16,
    c: Node<'a, 'a>,
    db: &mut AsyncPgConnection,
) -> Result<()> {
    let article = Article::get_or_create(
        get_req_text(c, "title")?,
        get_text_norm(c, "subtitle").as_deref(),
        get_text_norm(c, "note").as_deref(),
        db,
    )
    .await?;
    article.publish(issue.id, seqno, db).await?;
    for e in child_elems(c) {
        match e.tag_name().name() {
            "title" | "subtitle" | "note" => (), // handled above
            "by" => {
                let role = e.attribute("role").unwrap_or("by");
                for by in get_creators(e, db).await? {
                    use crate::schema::articles_by::dsl as ab;
                    diesel::insert_into(ab::articles_by)
                        .values((
                            ab::article_id.eq(article.id),
                            ab::creator_alias_id.eq(by.id),
                            ab::role.eq(role),
                        ))
                        .on_conflict_do_nothing()
                        .execute(db)
                        .await?;
                }
            }
            "ref" => article.set_refs(&parse_refs(e)?, db).await?,
            _ => return Err(unexpected_element(&e)),
        }
    }
    Ok(())
}

async fn register_serie<'a>(
    issue: &Issue,
    seqno: i16,
    c: Node<'a, 'a>,
    db: &mut AsyncPgConnection,
) -> Result<()> {
    use crate::schema::episodes::dsl as e;
    use crate::schema::episodes_by::dsl as eb;
    let episode = Episode::get_or_create(
        &Title::get_or_create(get_req_text(c, "title")?, db).await?,
        get_episode_name(c).as_deref(),
        get_text_norm(c, "teaser").as_deref(),
        get_text_norm(c, "note").as_deref(),
        get_text_norm(c, "copyright").as_deref(),
        db,
    )
    .await?;
    let part = get_child(c, "part");
    let part = Part {
        no: part
            .and_then(|p| parse_attribute(p, "no").transpose())
            .transpose()?,
        name: part.and_then(|p| p.text()).map(String::from),
    };
    Part::publish(
        &episode,
        &part,
        issue,
        Some(seqno),
        get_best_plac(c)?,
        &get_text_norm(c, "label").unwrap_or_default(),
        db,
    )
    .await?;
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
                    .execute(db)
                    .await?;
            }
            "label" | "title" | "episode" | "teaser" | "part" | "note"
            | "copyright" | "best" => (), // handled above
            "by" => {
                let role = e.attribute("role").unwrap_or("by");
                for by in get_creators(e, db).await? {
                    diesel::insert_into(eb::episodes_by)
                        .values((
                            eb::episode_id.eq(episode.id),
                            eb::creator_alias_id.eq(by.id),
                            eb::role.eq(role),
                        ))
                        .on_conflict_do_nothing()
                        .execute(db)
                        .await?;
                }
            }
            "ref" => episode
                .set_refs(&parse_refs(e)?, db)
                .await
                .with_context(|| format!("Error while handling {e:?}"))?,
            "prevpub" => {
                match e.first_element_child().map(|e| e.tag_name().name()) {
                    Some("fa") => {
                        let nr = parse_text(e, "fa")?.unwrap();
                        let year = parse_req_text(e, "year")?;
                        let issue =
                            Issue::get_or_create_ref(year, nr, db).await?;
                        Part::prevpub(&episode, &issue, db).await?;
                    }
                    Some("date") if e.attribute("role") == Some("orig") => {
                        let date: NaiveDate = parse_text(e, "date")?.unwrap();
                        diesel::update(e::episodes)
                            .set((
                                e::orig_date.eq(date),
                                e::orig_to_date.eq(Option::<NaiveDate>::None),
                                e::orig_sundays.eq(false),
                            ))
                            .filter(e::id.eq(episode.id))
                            .execute(db)
                            .await?;
                    }
                    Some("magazine") => {
                        let om = OtherMag::get_or_create(
                            get_text_norm(e, "magazine").unwrap(),
                            parse_text(e, "issue")?,
                            parse_text(e, "of")?,
                            parse_text(e, "year")?,
                            db,
                        )
                        .await?;
                        diesel::update(e::episodes)
                            .set(e::orig_mag_id.eq(om.id))
                            .filter(e::id.eq(episode.id))
                            .execute(db)
                            .await?;
                    }
                    _other => return Err(anyhow!("Unknown prevpub {:?}", e)),
                }
            }
            "daystrip" => {
                if let Some(from) = parse_text::<NaiveDate>(e, "from")? {
                    let to: NaiveDate = parse_req_text(e, "to")?;
                    let sun = e.attribute("d") == Some("sun");
                    diesel::update(e::episodes)
                        .set((
                            e::orig_date.eq(Some(from)),
                            e::orig_to_date.eq(Some(to)),
                            e::orig_sundays.eq(sun),
                        ))
                        .filter(e::id.eq(episode.id))
                        .execute(db)
                        .await?;
                } else if let Some(from) = parse_text::<i32>(e, "fromnr")? {
                    let to: i32 = parse_req_text(e, "tonr")?;
                    diesel::update(e::episodes)
                        .set((e::strip_from.eq(from), e::strip_to.eq(to)))
                        .filter(e::id.eq(episode.id))
                        .execute(db)
                        .await?;
                } else {
                    return Err(anyhow!("Unknown daystrip {:?}", e));
                }
            }
            _other => return Err(unexpected_element(&e)),
        }
    }
    Ok(())
}

fn get_episode_name(c: Node) -> Option<String> {
    if let Some(e) = get_child(c, "episode") {
        if e.attribute("role").is_none() {
            return e.text().map(normalize_space);
        }
    }
    None
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
    .map_err(|err| anyhow!("{} in {:?}", err, e))
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
    get_text(e, name).ok_or_else(|| anyhow!("{:?} missing child {}", e, name))
}

fn parse_text<T: FromStr>(e: Node, name: &str) -> Result<Option<T>>
where
    <T as FromStr>::Err: std::fmt::Display,
{
    get_child(e, name)
        .and_then(|e| e.text())
        .map(|t| {
            t.parse()
                .map_err(|e| anyhow!("Bad {name:?} element: {t:?}: {e}"))
        })
        .transpose()
}
fn parse_req_text<T: FromStr>(e: Node, name: &str) -> Result<T>
where
    <T as FromStr>::Err: std::fmt::Display,
{
    let t = get_child(e, name)
        .and_then(|e| e.text())
        .ok_or_else(|| anyhow!("{:?} missing text child {}", e, name))?;
    t.parse()
        .map_err(|e| anyhow!("Bad {name:?} element: {t:?}: {e}"))
}

fn get_text<'a>(e: Node<'a, 'a>, name: &str) -> Option<&'a str> {
    get_child(e, name).and_then(|e| e.text())
}

fn get_text_norm(e: Node, name: &str) -> Option<String> {
    get_text(e, name).map(normalize_space)
}

fn get_best_plac(e: Node) -> Result<Option<i16>> {
    get_child(e, "best")
        .and_then(|e| parse_attribute(e, "plac").transpose())
        .transpose()
}

async fn get_creators<'a>(
    by: Node<'a, 'a>,
    db: &mut AsyncPgConnection,
) -> Result<Vec<Creator>> {
    async fn one_creator<'a>(
        e: Node<'a, 'a>,
        db: &mut AsyncPgConnection,
    ) -> Result<Creator> {
        let name =
            e.text().ok_or_else(|| anyhow!("missing name in {e:?}"))?;
        Creator::get_or_create(name, db)
            .await
            .with_context(|| format!("Failed to create creator {name:?}"))
    }
    let mut who = Vec::new();
    for one in child_elems(by) {
        // Child <who> elements, each containing a name.
        who.push(one_creator(one, db).await?);
    }
    if who.is_empty() {
        // No child elements, a single name directly in the by element
        Ok(vec![one_creator(by, db).await?])
    } else {
        Ok(who)
    }
}

async fn read_persondata(
    base: &Path,
    db: &mut AsyncPgConnection,
) -> Result<()> {
    use crate::schema::creator_aliases::dsl as ca;
    use crate::schema::creators::dsl as c;
    let buf = read_to_string(base.join("extra-people.data"))?;
    for e in child_elems(Document::parse(&buf)?.root_element()) {
        match e.tag_name().name() {
            "person" => {
                let name = get_req_text(e, "name")?;
                let slug = e
                    .attribute("slug")
                    .map_or_else(|| slugify(name), String::from);
                let creator = c::creators
                    .select((c::id, c::name, c::slug))
                    .filter(c::name.eq(name))
                    .filter(c::slug.eq(&slug))
                    .first::<Creator>(db)
                    .await
                    .optional()?;
                let creator = if let Some(creator) = creator {
                    creator
                } else {
                    let c = diesel::insert_into(c::creators)
                        .values((c::name.eq(name), c::slug.eq(&slug)))
                        .returning((c::id, c::name, c::slug))
                        .get_result::<Creator>(db)
                        .await?;
                    diesel::insert_into(ca::creator_aliases)
                        .values((ca::creator_id.eq(c.id), ca::name.eq(name)))
                        .execute(db)
                        .await?;
                    c
                };

                for a in
                    e.children().filter(|a| a.tag_name().name() == "alias")
                {
                    if let Some(alias) = a.text() {
                        match ca::creator_aliases
                            .select(ca::id)
                            .filter(ca::creator_id.eq(creator.id))
                            .filter(ca::name.eq(alias))
                            .first::<i32>(db)
                            .await
                            .optional()?
                        {
                            Some(_) => (),
                            None => {
                                diesel::insert_into(ca::creator_aliases)
                                    .values((
                                        ca::creator_id.eq(creator.id),
                                        ca::name.eq(alias),
                                    ))
                                    //.returning(ca::id)
                                    .execute(db)
                                    .await?;
                            }
                        }
                    }
                }
            }
            _ => return Err(unexpected_element(&e)),
        }
    }
    Ok(())
}

async fn delete_unpublished(db: &mut AsyncPgConnection) -> Result<()> {
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

    do_clear(db, "episode parts", {
        let published_parts = p::publications
            .select(p::episode_part)
            .filter(p::episode_part.is_not_null())
            .distinct();
        ep::episode_parts.filter(ep::id.nullable().ne_all(published_parts))
    })
    .await?;

    do_clear(db, "episode refkeys", {
        er::episode_refkeys.filter(er::episode_id.eq_any(
            e::episodes.select(e::id).filter(
                e::id.ne_all(
                    ep::episode_parts.select(ep::episode_id).distinct(),
                ),
            ),
        ))
    })
    .await?;

    do_clear(db, "episodes-by", {
        eb::episodes_by.filter(eb::episode_id.eq_any(
            e::episodes.select(e::id).filter(
                e::id.ne_all(
                    ep::episode_parts.select(ep::episode_id).distinct(),
                ),
            ),
        ))
    })
    .await?;

    do_clear(db, "episodes", {
        e::episodes.filter(
            e::id.ne_all(ep::episode_parts.select(ep::episode_id).distinct()),
        )
    })
    .await?;

    do_clear(db, "titles", {
        t::titles
            .filter(t::id.ne_all(e::episodes.select(e::title_id).distinct()))
    })
    .await?;

    let start = Instant::now();
    let published_articles = p::publications
        .filter(p::article_id.is_not_null())
        .select(p::article_id)
        .distinct()
        .load::<Option<i32>>(db)
        .await?
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();

    do_clear(db, "article refkeys", {
        ar::article_refkeys.filter(ar::article_id.ne_all(&published_articles))
    })
    .await?;

    do_clear(db, "articles-by", {
        ab::articles_by.filter(ab::article_id.ne_all(&published_articles))
    })
    .await?;

    do_clear(db, "articles", {
        a::articles.filter(a::id.ne_all(&published_articles))
    })
    .await?;

    println!(
        "Article-related cleanups in {:.0?} ({} remains).",
        start.elapsed(),
        published_articles.len(),
    );

    do_clear(db, "refkeys", {
        r::refkeys
            .filter(r::id.ne_all(er::episode_refkeys.select(er::refkey_id)))
            .filter(r::id.ne_all(ar::article_refkeys.select(ar::refkey_id)))
    })
    .await?;

    Ok(())
}

async fn do_clear<'a, T: 'a + IntoUpdateTarget>(
    db: &'a mut AsyncPgConnection,
    what: &'static str,
    how: T,
) -> Result<()>
where
    <T as IntoUpdateTarget>::WhereClause:
        QueryFragment<Pg> + QueryId + Send + Sync,
    <T as HasTable>::Table: QueryId + Send + Sync + 'static,
    <<T as HasTable>::Table as QuerySource>::FromClause:
        QueryFragment<Pg> + Send + Sync,
{
    let start = Instant::now();
    let n = diesel::delete(how).execute(db).await.context(what)?;
    println!("Cleared junk {} {} in {:.0?}", n, what, start.elapsed());
    Ok(())
}

fn get_child<'a>(node: Node<'a, 'a>, name: &str) -> Option<Node<'a, 'a>> {
    node.children().find(|a| a.tag_name().name() == name)
}

fn child_elems<'a, 'b>(
    node: Node<'a, 'b>,
) -> impl Iterator<Item = Node<'a, 'b>> {
    node.children().filter(Node::is_element)
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

use std::str::FromStr;
fn parse_attribute<T: FromStr>(e: Node, name: &str) -> Result<Option<T>>
where
    <T as std::str::FromStr>::Err: 'static + Send + Sync + std::error::Error,
{
    e.attribute(name)
        .map(str::parse)
        .transpose()
        .with_context(|| format!("Bad attribute {name:?}"))
}

fn unexpected_element(e: &Node) -> anyhow::Error {
    anyhow!("Unexpected element {e:?}")
}
