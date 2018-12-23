use crate::schema;
use crate::templates::ToHtml;
use diesel;
use diesel::pg::{Pg, PgConnection};
use diesel::prelude::*;
use diesel::result::Error;
use slug::slugify;
use std::io::{self, Write};

#[derive(Debug)]
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
            RefKey::Fa(s) => (RefKey::FA_ID, "", s),
            RefKey::Key(t, s) => (RefKey::KEY_ID, t.as_ref(), s),
            RefKey::Who(n, s) => (RefKey::WHO_ID, n.as_ref(), s),
            RefKey::Title(n, s) => (RefKey::TITLE_ID, n.as_ref(), s),
        };
        use crate::schema::refkeys::dsl;
        dsl::refkeys
            .select(dsl::id)
            .filter(dsl::kind.eq(kind))
            .filter(dsl::title.eq(title))
            .filter(dsl::slug.eq(slug))
            .first(db)
            .optional()?
            .ok_or(0)
            .or_else(|_| {
                diesel::insert_into(dsl::refkeys)
                    .values((
                        dsl::kind.eq(kind),
                        dsl::title.eq(title),
                        dsl::slug.eq(slug),
                    ))
                    .returning(dsl::id)
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
}

impl Queryable<schema::refkeys::SqlType, Pg> for RefKey {
    type Row = (i32, i16, Option<String>, String);

    fn build(row: Self::Row) -> Self {
        match row {
            (_, RefKey::KEY_ID, Some(t), s) => RefKey::Key(t, s),
            (_, RefKey::FA_ID, _, s) => RefKey::Fa(s),
            (_, RefKey::WHO_ID, Some(t), s) => RefKey::Who(t, s),
            (_, RefKey::TITLE_ID, Some(t), s) => RefKey::Title(t, s),
            (id, k, t, s) => {
                panic!("Bad refkey #{} kind {} ({:?}, {:?})", id, k, t, s)
            }
        }
    }
}

impl ToHtml for RefKey {
    fn to_html(&self, out: &mut Write) -> io::Result<()> {
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
