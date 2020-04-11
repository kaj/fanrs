use super::{Article, Creator, Episode};
use crate::server::PgPool;
use crate::templates::ToHtml;
use diesel::prelude::*;
use diesel::result::Error;
use std::collections::BTreeMap;
use std::io::{self, Write};
use tokio_diesel::{AsyncError, AsyncRunQueryDsl};

#[derive(Debug)]
pub struct CreatorSet(BTreeMap<String, Vec<Creator>>);

impl CreatorSet {
    pub const MAIN_ROLES: &'static [&'static str] =
        &["by", "bild", "text", "orig", "ink"];

    pub fn for_episode(
        episode: &Episode,
        db: &PgConnection,
    ) -> Result<CreatorSet, Error> {
        use crate::schema::creator_aliases::dsl as ca;
        use crate::schema::creators::dsl as c;
        use crate::schema::episodes_by::dsl as cp;
        let c_columns = (c::id, ca::name, c::slug);
        let data = cp::episodes_by
            .inner_join(ca::creator_aliases.inner_join(c::creators))
            .select((cp::role, c_columns))
            .filter(cp::episode_id.eq(episode.id))
            .load::<(String, Creator)>(db)?;
        Ok(CreatorSet::from_data(data))
    }
    pub async fn for_episode_async(
        episode: &Episode,
        db: &PgPool,
    ) -> Result<CreatorSet, AsyncError> {
        use crate::schema::creator_aliases::dsl as ca;
        use crate::schema::creators::dsl as c;
        use crate::schema::episodes_by::dsl as cp;
        let c_columns = (c::id, ca::name, c::slug);
        let data = cp::episodes_by
            .inner_join(ca::creator_aliases.inner_join(c::creators))
            .select((cp::role, c_columns))
            .filter(cp::episode_id.eq(episode.id))
            .load_async::<(String, Creator)>(db)
            .await?;
        Ok(CreatorSet::from_data(data))
    }

    pub fn for_article(
        article: &Article,
        db: &PgConnection,
    ) -> Result<CreatorSet, Error> {
        use crate::schema::articles_by::dsl as ab;
        use crate::schema::creator_aliases::dsl as ca;
        use crate::schema::creators::dsl as c;
        let c_columns = (c::id, ca::name, c::slug);
        let data = ab::articles_by
            .inner_join(ca::creator_aliases.inner_join(c::creators))
            .select((ab::role, c_columns))
            .filter(ab::article_id.eq(article.id))
            .load::<(String, Creator)>(db)?;
        Ok(CreatorSet::from_data(data))
    }
    pub async fn for_article_async(
        article: &Article,
        db: &PgPool,
    ) -> Result<CreatorSet, AsyncError> {
        use crate::schema::articles_by::dsl as ab;
        use crate::schema::creator_aliases::dsl as ca;
        use crate::schema::creators::dsl as c;
        let c_columns = (c::id, ca::name, c::slug);
        let data = ab::articles_by
            .inner_join(ca::creator_aliases.inner_join(c::creators))
            .select((ab::role, c_columns))
            .filter(ab::article_id.eq(article.id))
            .load_async::<(String, Creator)>(db)
            .await?;
        Ok(CreatorSet::from_data(data))
    }

    fn from_data(data: Vec<(String, Creator)>) -> CreatorSet {
        let mut result: BTreeMap<String, Vec<Creator>> = BTreeMap::new();
        for (role, creator) in data {
            result.entry(role).or_insert_with(|| vec![]).push(creator);
        }
        CreatorSet(result)
    }
}

impl ToHtml for CreatorSet {
    fn to_html(&self, out: &mut dyn Write) -> io::Result<()> {
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
