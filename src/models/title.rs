use super::{Cloud, CloudItem};
use crate::schema::episode_parts::dsl as ep;
use crate::schema::episodes::dsl as e;
use crate::schema::titles::{self, dsl as t};
use crate::templates::ToHtml;
use diesel::prelude::*;
use diesel::result::Error;
use diesel_async::{AsyncPgConnection, RunQueryDsl};
use slug::slugify;
use std::cmp::Ordering;
use std::io::{self, Write};

/// A title of a comic.
///
/// May be recurring, such as "Fantomen" or "Spirit", or a one-shot.
#[derive(Debug, Queryable, Selectable, PartialEq, Eq)]
pub struct Title {
    pub id: i32,
    pub title: String,
    pub slug: String,
}

impl Title {
    pub async fn get_or_create(
        name: &str,
        db: &mut AsyncPgConnection,
    ) -> Result<Title, Error> {
        if let Some(t) = t::titles
            .filter(t::title.eq(name))
            .first::<Title>(db)
            .await
            .optional()?
        {
            Ok(t)
        } else {
            Ok(diesel::insert_into(t::titles)
                .values((t::title.eq(name), t::slug.eq(&slugify(name))))
                .get_result(db)
                .await?)
        }
    }

    pub async fn from_slug(
        slug: String,
        db: &mut AsyncPgConnection,
    ) -> Result<Title, Error> {
        t::titles.filter(t::slug.eq(slug)).first(db).await
    }

    pub fn has_daystrip(&self) -> bool {
        let t: &str = &self.slug;
        DAYSTRIPS.binary_search(&t).is_ok()
    }
    pub fn has_sundays(&self) -> bool {
        SUNDAYS.binary_search(&self.slug.as_ref()).is_ok()
    }
    pub async fn cloud(
        num: i64,
        db: &mut AsyncPgConnection,
    ) -> Result<Cloud<Title>, Error> {
        use diesel::dsl::sql;
        use diesel::sql_types::Integer;
        let c = sql::<Integer>("cast(count(*) as integer)");
        let titles = t::titles
            .left_join(e::episodes.left_join(ep::episode_parts))
            .select((t::titles::all_columns(), c.clone()))
            .group_by(t::titles::all_columns())
            .order(c.desc())
            .limit(num)
            .load(db)
            .await?;
        Ok(Cloud::from_ordered(titles))
    }
}

impl PartialOrd for Title {
    fn partial_cmp(&self, rhs: &Title) -> Option<Ordering> {
        Some(self.cmp(rhs))
    }
}

impl Ord for Title {
    fn cmp(&self, rhs: &Title) -> Ordering {
        self.title.cmp(&rhs.title)
    }
}

impl CloudItem for Title {
    fn write_item(
        &self,
        out: &mut dyn Write,
        n: i32,
        w: u8,
    ) -> io::Result<()> {
        write!(
            out,
            "<a href='/titles/{}' class='w{}' data-n='{}'>",
            self.slug, w, n,
        )?;
        self.title.to_html(out)?;
        write!(out, "</a>")
    }
}

static DAYSTRIPS: [&str; 7] = [
    "blixt-gordon",
    "fantomen",
    "johnny-hazard",
    "king-vid-granspolisen",
    "latigo",
    "mandrake",
    "rick-o-shay",
];

static SUNDAYS: [&str; 5] = [
    "fantomen",
    "johnny-hazard",
    "ludvig",
    "mandrake",
    "mandrake-fantomen",
];

#[test]
fn test_daystrips_sorted() {
    assert!(is_sorted(&DAYSTRIPS));
}
#[test]
fn test_sundays_sorted() {
    assert!(is_sorted(&SUNDAYS));
}
#[cfg(test)]
fn is_sorted(data: &[&str]) -> bool {
    data.windows(2).all(|w| w[0] < w[1])
}
