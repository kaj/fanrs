use super::{Cloud, CloudItem};
use crate::schema;
use crate::schema::episode_refkeys::dsl as er;
use crate::schema::episodes::dsl as e;
use crate::schema::refkeys::dsl as r;
use crate::server::PgPool;
use crate::templates::ToHtml;
use diesel::dsl::sql;
use diesel::pg::{Pg, PgConnection};
use diesel::prelude::*;
use diesel::result::Error;
use diesel::sql_types::{Integer, SmallInt, Text};
use slug::slugify;
use std::cmp::Ordering;
use std::io::{self, Write};
use tokio_diesel::{AsyncError, AsyncRunQueryDsl};

#[derive(Debug)]
pub struct IdRefKey {
    pub id: i32,
    pub refkey: RefKey,
}

impl IdRefKey {
    pub async fn key_from_slug(
        slug: String,
        db: &PgPool,
    ) -> Result<Self, AsyncError> {
        IdRefKey::from_slug_async(slug, RefKey::KEY_ID, db).await
    }
    pub async fn fa_from_slug(
        slug: String,
        db: &PgPool,
    ) -> Result<Self, AsyncError> {
        IdRefKey::from_slug_async(slug, RefKey::FA_ID, db).await
    }
    async fn from_slug_async(
        slug: String,
        kind: i16,
        db: &PgPool,
    ) -> Result<Self, AsyncError> {
        r::refkeys
            .select(r::refkeys::all_columns())
            .filter(r::kind.eq(kind))
            .filter(r::slug.eq(slug))
            .first_async(db)
            .await
    }
    pub fn name(&self) -> String {
        self.refkey.name()
    }
    pub fn slug(&self) -> &str {
        self.refkey.slug()
    }
    pub fn letter(&self) -> char {
        self.refkey.letter()
    }
}

#[derive(Debug, Eq, PartialEq)]
pub enum RefKey {
    /// slug
    Fa(String),
    /// Actual key and slug
    Key(String, String),
    /// Name, slug
    Who(String, String),
    /// Name, slug
    // Note; In some sense, this should actually reference a title by id,
    // but this way it can be stored the same way as other refkeys.
    Title(String, String),
}

impl RefKey {
    pub const FA_ID: i16 = 2;
    pub const KEY_ID: i16 = 1;
    pub const WHO_ID: i16 = 3;
    pub const TITLE_ID: i16 = 4;

    pub fn fa(slug: &str) -> RefKey {
        RefKey::Fa(slug.into())
    }
    pub fn key(name: &str) -> RefKey {
        match name {
            "Julie" => RefKey::fa("17j"),
            // To be replaced with both 21.1 and 21.2 somehow
            "Kit & Heloise" => Self::fa("22"),
            "Kit" => Self::fa("22k"),
            "Heloise" => Self::fa("22h"),
            _ => RefKey::Key(name.into(), slugify(name)),
        }
    }
    pub fn who(name: &str) -> RefKey {
        RefKey::Who(name.into(), slugify(name))
    }
    pub fn title(name: &str) -> RefKey {
        RefKey::Title(name.into(), slugify(name))
    }

    pub fn get_or_create_id(&self, db: &PgConnection) -> Result<i32, Error> {
        let (kind, title, slug) = match self {
            RefKey::Fa(s) => (RefKey::FA_ID, self.name(), s.clone()),
            RefKey::Key(t, s) => (RefKey::KEY_ID, t.clone(), s.clone()),
            RefKey::Who(n, _s) => {
                use super::Creator;
                let alias = Creator::get_or_create(&n, db)?;
                let actual = Creator::from_slug(&alias.slug, db)?;
                (RefKey::WHO_ID, actual.name, actual.slug)
            }
            RefKey::Title(n, s) => (RefKey::TITLE_ID, n.clone(), s.clone()),
        };
        r::refkeys
            .select(r::id)
            .filter(r::kind.eq(kind))
            .filter(r::title.eq(&title))
            .filter(r::slug.eq(&slug))
            .first(db)
            .optional()?
            .ok_or(0)
            .or_else(|_| {
                diesel::insert_into(r::refkeys)
                    .values((
                        r::kind.eq(kind),
                        r::title.eq(title),
                        r::slug.eq(&slug),
                    ))
                    .returning(r::id)
                    .get_result::<i32>(db)
            })
    }

    pub fn url(&self) -> String {
        match self {
            RefKey::Fa(slug) => format!("/fa/{}", slug),
            RefKey::Key(_, slug) => format!("/what/{}", slug),
            RefKey::Who(_, slug) => format!("/who/{}", slug),
            RefKey::Title(_, slug) => format!("/titles/{}", slug),
        }
    }
    pub fn slug(&self) -> &str {
        match self {
            RefKey::Fa(slug) => slug,
            RefKey::Key(_, slug) => slug,
            RefKey::Who(_, slug) => slug,
            RefKey::Title(_, slug) => slug,
        }
    }

    pub fn name(&self) -> String {
        match self {
            RefKey::Fa(slug) => match slug.as_ref() {
                "0" => "Kapten Walker".into(),
                "1" => "Den 1:a Fantomen".into(),
                "17j" => "Julie".into(),
                "22h" => "Heloise".into(),
                "22k" => "Kit".into(),
                slug => format!("Den {}:e Fantomen", slug),
            },
            RefKey::Key(name, _) => name.clone(),
            RefKey::Who(name, _) => name.clone(),
            RefKey::Title(name, _) => name.clone(),
        }
    }

    pub fn short(&self) -> String {
        match self {
            RefKey::Fa(slug) => match slug.as_ref() {
                "0" => "Kapten Walker".into(),
                "17j" => "Julie".into(),
                "22h" => "Heloise".into(),
                "22k" => "Kit".into(),
                slug => slug.to_string(),
            },
            RefKey::Key(name, _) => name.clone(),
            RefKey::Who(name, _) => name.clone(),
            RefKey::Title(name, _) => name.clone(),
        }
    }
    pub fn letter(&self) -> char {
        match self {
            RefKey::Fa(..) => 'f',
            RefKey::Key(..) => 'k',
            RefKey::Who(..) => 'p',
            RefKey::Title(..) => 't',
        }
    }

    pub async fn cloud(
        num: i64,
        db: &PgPool,
    ) -> Result<Cloud<RefKey>, AsyncError> {
        let c = sql::<Integer>("cast(count(*) as integer)");
        let refkeys = r::refkeys
            .left_join(er::episode_refkeys.left_join(e::episodes))
            .select(((r::kind, r::title, r::slug), c.clone()))
            .filter(r::kind.eq(RefKey::KEY_ID))
            .group_by(r::refkeys::all_columns())
            .order(c.desc())
            .limit(num)
            .load_async(db)
            .await?;
        Ok(Cloud::from_ordered(refkeys))
    }
}

impl Queryable<schema::refkeys::SqlType, Pg> for IdRefKey {
    type Row = (i32, i16, String, String);

    fn build(row: Self::Row) -> Self {
        IdRefKey {
            id: row.0,
            refkey: match (row.1, row.2, row.3) {
                (RefKey::KEY_ID, t, s) => RefKey::Key(t, s),
                (RefKey::FA_ID, _, s) => RefKey::Fa(s),
                (RefKey::WHO_ID, t, s) => RefKey::Who(t, s),
                (RefKey::TITLE_ID, t, s) => RefKey::Title(t, s),
                (k, t, s) => panic!(
                    "Bad refkey #{} kind {} ({:?}, {:?})",
                    row.0, k, t, s,
                ),
            },
        }
    }
}

impl Queryable<(SmallInt, Text, Text), Pg> for RefKey {
    type Row = (i16, String, String);

    fn build(row: Self::Row) -> Self {
        match row {
            (RefKey::KEY_ID, t, s) => RefKey::Key(t, s),
            (RefKey::FA_ID, _, s) => RefKey::Fa(s),
            (RefKey::WHO_ID, t, s) => RefKey::Who(t, s),
            (RefKey::TITLE_ID, t, s) => RefKey::Title(t, s),
            (k, t, s) => panic!("Bad refkey kind {} ({:?}, {:?})", k, t, s),
        }
    }
}

impl ToHtml for RefKey {
    fn to_html(&self, out: &mut dyn Write) -> io::Result<()> {
        out.write_all(b"<a href=\"")?;
        self.url().to_html(out)?;
        write!(
            out,
            "\" class=\"ref {}\">",
            match self {
                RefKey::Fa(..) => "fa",
                RefKey::Key(..) => "key",
                RefKey::Who(..) => "who",
                RefKey::Title(..) => "title",
            }
        )?;
        self.name().to_html(out)?;
        out.write_all(b"</a>")
    }
}

impl Ord for RefKey {
    fn cmp(&self, rhs: &RefKey) -> Ordering {
        // Note: Should sort by kind first, but only used inside same kind.
        self.name().cmp(&rhs.name())
    }
}
impl PartialOrd for RefKey {
    fn partial_cmp(&self, rhs: &RefKey) -> Option<Ordering> {
        Some(self.cmp(rhs))
    }
}

impl CloudItem for RefKey {
    fn write_item(
        &self,
        out: &mut dyn Write,
        n: i32,
        w: u8,
    ) -> io::Result<()> {
        write!(
            out,
            "<a href='{}' class='w{}' data-n='{}'>",
            self.url(),
            w,
            n,
        )?;
        self.name().to_html(out)?;
        write!(out, "</a>")
    }
}
