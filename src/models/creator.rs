use diesel::prelude::*;
use diesel::result::Error;
use slug::slugify;
use std::io::{self, Write};
use templates::ToHtml;

/// In most cases, this struct will hold the id and name from
/// creator_aliases together with the slug from creators.
#[derive(Debug, Queryable)]
pub struct Creator {
    pub id: i32,
    pub name: String,
    pub slug: String,
}

impl Creator {
    pub fn get_or_create(
        name: &str,
        db: &PgConnection,
    ) -> Result<Creator, Error> {
        use schema::creator_aliases::dsl as ca;
        use schema::creators::dsl as c;
        if let Some(t) = c::creators
            .inner_join(ca::creator_aliases)
            .select((ca::id, ca::name, c::slug))
            .filter(ca::name.eq(name))
            .first(db)
            .optional()?
        {
            Ok(t)
        } else {
            let slug = slugify(name);
            let creator: Creator = diesel::insert_into(c::creators)
                .values((c::name.eq(name), c::slug.eq(slug)))
                .get_result(db)?;
            diesel::insert_into(ca::creator_aliases)
                .values((ca::creator_id.eq(creator.id), ca::name.eq(name)))
                .execute(db)?;
            Ok(creator)
        }
    }
}

impl ToHtml for Creator {
    fn to_html(&self, out: &mut Write) -> io::Result<()> {
        write!(out, "<a href='/who/{}'>{}</a>", self.slug, self.name)
    }
}
