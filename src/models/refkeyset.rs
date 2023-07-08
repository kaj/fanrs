use super::{Article, Episode, IdRefKey, RefKey};
use crate::templates::ToHtml;
use diesel::prelude::*;
use diesel::result::Error;
use diesel_async::{AsyncPgConnection, RunQueryDsl};
use std::io::{self, Write};

#[derive(Debug)]
pub struct RefKeySet(Vec<RefKey>);

impl RefKeySet {
    pub async fn for_article(
        article: &Article,
        db: &mut AsyncPgConnection,
    ) -> Result<RefKeySet, Error> {
        use crate::schema::article_refkeys::dsl as ar;
        use crate::schema::refkeys::dsl as r;
        Ok(RefKeySet(
            r::refkeys
                .inner_join(ar::article_refkeys)
                .select(IdRefKey::as_select())
                .filter(ar::article_id.eq(article.id))
                .order((r::title, r::slug))
                .load::<IdRefKey>(db)
                .await?
                .into_iter()
                .map(|ir| ir.refkey)
                .collect(),
        ))
    }

    pub async fn for_episode(
        episode: &Episode,
        db: &mut AsyncPgConnection,
    ) -> Result<RefKeySet, Error> {
        use crate::schema::episode_refkeys::dsl as er;
        use crate::schema::refkeys::dsl as r;
        Ok(RefKeySet(
            r::refkeys
                .inner_join(er::episode_refkeys)
                .select(IdRefKey::as_select())
                .filter(er::episode_id.eq(episode.id))
                .order((r::title, r::slug))
                .load::<IdRefKey>(db)
                .await?
                .into_iter()
                .map(|ir| ir.refkey)
                .collect(),
        ))
    }
}

impl ToHtml for RefKeySet {
    fn to_html(&self, out: &mut dyn Write) -> io::Result<()> {
        if let Some((last_ref, refs)) = self.0.split_last() {
            out.write_all(b"<p class='info refs'>Referenser: ")?;
            for r in refs {
                r.to_html(out)?;
                out.write_all(b", ")?;
            }
            last_ref.to_html(out)?;
            out.write_all(b".</p>")?;
        }
        Ok(())
    }
}
