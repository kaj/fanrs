use super::{Article, Creator, Episode};
use crate::templates::ToHtml;
use diesel::prelude::*;
use diesel::result::Error;
use diesel_async::{AsyncPgConnection, RunQueryDsl};
use std::collections::BTreeMap;
use std::io::{self, Write};

#[derive(Debug)]
pub struct CreatorSet(BTreeMap<String, Vec<Creator>>);

impl CreatorSet {
    pub const MAIN_ROLES: &'static [&'static str] =
        &["by", "bild", "text", "orig", "ink"];

    pub async fn for_episode(
        episode: &Episode,
        db: &mut AsyncPgConnection,
    ) -> Result<CreatorSet, Error> {
        use crate::schema::creator_aliases::dsl as ca;
        use crate::schema::creators::dsl as c;
        use crate::schema::episodes_by::dsl as cp;
        let c_columns = (c::id, ca::name, c::slug);
        let data = cp::episodes_by
            .inner_join(ca::creator_aliases.inner_join(c::creators))
            .select((cp::role, c_columns))
            .filter(cp::episode_id.eq(episode.id))
            .load::<(String, Creator)>(db)
            .await?;
        Ok(CreatorSet::from_data(data))
    }

    pub async fn for_article(
        article: &Article,
        db: &mut AsyncPgConnection,
    ) -> Result<CreatorSet, Error> {
        use crate::schema::articles_by::dsl as ab;
        use crate::schema::creator_aliases::dsl as ca;
        use crate::schema::creators::dsl as c;
        let c_columns = (c::id, ca::name, c::slug);
        let data = ab::articles_by
            .inner_join(ca::creator_aliases.inner_join(c::creators))
            .select((ab::role, c_columns))
            .filter(ab::article_id.eq(article.id))
            .load::<(String, Creator)>(db)
            .await?;
        Ok(CreatorSet::from_data(data))
    }

    fn from_data(data: Vec<(String, Creator)>) -> CreatorSet {
        let mut result: BTreeMap<String, Vec<Creator>> = BTreeMap::new();
        for (role, creator) in data {
            result.entry(role).or_default().push(creator);
        }
        CreatorSet(result)
    }
}

impl ToHtml for CreatorSet {
    fn to_html(&self, out: &mut dyn Write) -> io::Result<()> {
        if !self.0.is_empty() {
            write!(out, "<p class='info creators'>")?;
            let roles = [
                ("by", "Av"),
                ("text", "Text:"),
                ("bild", "Bild:"),
                ("ink", "Tush:"),
                ("color", "Färgläggning:"),
                ("orig", "Efter en originalberättelse av:"),
                ("redax", "Redaktion:"),
                ("xlat", "Översättning:"),
                ("textning", "Textsättning:"),
            ];
            for (code, desc) in roles {
                if let Some((last, rest)) =
                    self.0.get(code).and_then(|s| s.split_last())
                {
                    write!(out, "{desc} ")?;
                    if let Some((first, rest)) = rest.split_first() {
                        first.to_html(out)?;
                        for creator in rest {
                            write!(out, ", ")?;
                            creator.to_html(out)?;
                        }
                        write!(out, " och ")?;
                    }
                    last.to_html(out)?;
                    write!(out, ". ")?;
                }
            }
            write!(out, "</p>")?;
        }
        Ok(())
    }
}
