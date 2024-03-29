use super::{Cloud, CloudItem};
use crate::schema::creator_aliases::dsl as ca;
use crate::schema::creators::dsl as c;
use crate::templates::ToHtml;
use diesel::prelude::*;
use diesel::result::Error;
use diesel_async::{AsyncPgConnection, RunQueryDsl};
use slug::slugify;
use std::cmp::Ordering;
use std::io::{self, Write};

/// In most cases, this struct will hold the id and name from
/// `creator_aliases` together with the slug from creators.
#[derive(Debug, Queryable, Eq, PartialEq)]
pub struct Creator {
    pub id: i32,
    pub name: String,
    pub slug: String,
}

impl Creator {
    /// The id and name here is for an alias.
    pub async fn get_or_create(
        name: &str,
        db: &mut AsyncPgConnection,
    ) -> Result<Creator, Error> {
        if let Some(t) = c::creators
            .inner_join(ca::creator_aliases)
            .select((ca::id, ca::name, c::slug))
            .filter(ca::name.eq(name))
            .first(db)
            .await
            .optional()?
        {
            Ok(t)
        } else {
            let slug = slugify(name);
            let mut creator: Creator = diesel::insert_into(c::creators)
                .values((c::name.eq(name), c::slug.eq(slug)))
                .get_result(db)
                .await?;
            creator.id = diesel::insert_into(ca::creator_aliases)
                .values((ca::creator_id.eq(creator.id), ca::name.eq(name)))
                .returning(ca::id)
                .get_result(db)
                .await?;
            Ok(creator)
        }
    }

    /// The id and name here is for the actual creator.
    pub async fn from_slug(
        slug: &str,
        db: &mut AsyncPgConnection,
    ) -> Result<Creator, Error> {
        c::creators
            .select((c::id, c::name, c::slug))
            .filter(c::slug.eq(slug))
            .first(db)
            .await
    }

    pub async fn cloud(
        num: i64,
        db: &mut AsyncPgConnection,
    ) -> Result<Cloud<Creator>, Error> {
        use crate::models::creator_contributions::creator_contributions::dsl as cc;
        let creators = cc::creator_contributions
            .select(((cc::id, cc::name, cc::slug), cc::score))
            .order_by(cc::score.desc())
            .limit(num)
            .load(db)
            .await?;
        Ok(Cloud::from_ordered(creators))
    }
}

impl ToHtml for Creator {
    fn to_html(&self, out: &mut dyn Write) -> io::Result<()> {
        write!(out, "<a href='/who/{}'>{}</a>", self.slug, self.name)
    }
}

impl CloudItem for Creator {
    fn write_item(
        &self,
        out: &mut dyn Write,
        n: i32,
        w: u8,
    ) -> io::Result<()> {
        write!(
            out,
            "<a href='/who/{}' class='w{}' data-n='{}'>",
            self.slug, w, n,
        )?;
        self.name.to_html(out)?;
        write!(out, "</a>")
    }
}

impl PartialOrd for Creator {
    fn partial_cmp(&self, rhs: &Creator) -> Option<Ordering> {
        Some(self.cmp(rhs))
    }
}

impl Ord for Creator {
    fn cmp(&self, rhs: &Creator) -> Ordering {
        self.name.cmp(&rhs.name)
    }
}
