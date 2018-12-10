use super::{Creator, Episode};
use crate::templates::ToHtml;
use diesel::prelude::*;
use failure::Error;
use std::collections::BTreeMap;
use std::io::{self, Write};

#[derive(Debug)]
pub struct CreatorSet(BTreeMap<String, Vec<Creator>>);

impl CreatorSet {
    pub fn for_episode(
        episode: &Episode,
        db: &PgConnection,
    ) -> Result<CreatorSet, Error> {
        use crate::schema::creativeparts::dsl as cp;
        use crate::schema::creator_aliases::dsl as ca;
        use crate::schema::creators::dsl as c;
        let c_columns = (c::id, ca::name, c::slug);
        let data = cp::creativeparts
            .inner_join(ca::creator_aliases.inner_join(c::creators))
            .select((cp::role, c_columns))
            .filter(cp::episode_id.eq(episode.id))
            .load::<(String, Creator)>(db)?;
        let mut result: BTreeMap<String, Vec<Creator>> = BTreeMap::new();
        for (role, creator) in data {
            result.entry(role).or_insert_with(|| vec![]).push(creator);
        }
        Ok(CreatorSet(result))
    }
}

impl ToHtml for CreatorSet {
    fn to_html(&self, out: &mut Write) -> io::Result<()> {
        if !self.0.is_empty() {
            write!(out, "<p class='info creators'>")?;
            let roles = [
                ("by".to_string(), "Av"),
                ("text".to_string(), "Text:"),
                ("bild".to_string(), "Bild:"),
                ("ink".to_string(), "Tush:"),
                ("color".to_string(), "Färgläggning:"),
                ("orig".to_string(), "Efter en originalberättelse av:"),
                ("redax".to_string(), "Redaktion:"),
                ("xlat".to_string(), "Översättning:"),
                ("textning".to_string(), "Textsättning:"),
            ];
            for (code, desc) in &roles {
                if let Some(creators) = self.0.get(code) {
                    write!(out, "{} ", desc)?;
                    if let Some((last, rest)) = creators.split_last() {
                        if let Some((first, rest)) = rest.split_first() {
                            first.to_html(out)?;
                            for creator in rest {
                                write!(out, ", ")?;
                                creator.to_html(out)?;
                            }
                            write!(out, " och ")?;
                        }
                        last.to_html(out)?;
                    }
                    write!(out, ". ")?;
                }
            }
            write!(out, "</p>")?;
        }
        Ok(())
    }
}
