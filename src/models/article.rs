use super::RefKey;
use crate::schema::article_refkeys::dsl as ar;
use crate::schema::articles::{self, dsl as a};
use crate::schema::publications::dsl as p;
use diesel::prelude::*;
use diesel::result::Error;
use diesel_async::{AsyncPgConnection, RunQueryDsl};
use tracing::warn;

#[derive(Debug, Queryable, Selectable, PartialEq, Eq)]
pub struct Article {
    pub id: i32,
    pub title: String,
    pub subtitle: Option<String>,
    pub note: Option<String>,
}

impl Article {
    pub async fn get_or_create(
        title: &str,
        subtitle: Option<&str>,
        note: Option<&str>,
        db: &mut AsyncPgConnection,
    ) -> Result<Article, Error> {
        if let Some(article) = a::articles
            .filter(a::title.eq(title))
            .filter(a::subtitle.is_not_distinct_from(subtitle))
            .filter(a::note.is_not_distinct_from(note))
            .first::<Article>(db)
            .await
            .optional()?
        {
            Ok(article)
        } else {
            Ok(diesel::insert_into(a::articles)
                .values((
                    a::title.eq(title),
                    a::subtitle.eq(subtitle),
                    a::note.eq(note),
                ))
                .get_result(db)
                .await?)
        }
    }

    /// This article is published in a specific issue.
    pub async fn publish(
        &self,
        issue: i32,
        seqno: i16,
        db: &mut AsyncPgConnection,
    ) -> Result<(), Error> {
        if let Some((id, old)) = p::publications
            .filter(p::issue_id.eq(issue))
            .filter(p::article_id.eq(self.id))
            .select((p::id, p::seqno))
            .first::<(i32, Option<i16>)>(db)
            .await
            .optional()?
        {
            if old != Some(seqno) {
                warn!(
                    "TODO: Update seqno for article #{id} ({old:?} != {seqno})",
                );
            }
            Ok(())
        } else {
            diesel::insert_into(p::publications)
                .values((
                    p::issue_id.eq(issue),
                    p::article_id.eq(self.id),
                    p::seqno.eq(seqno),
                ))
                .execute(db)
                .await?;
            Ok(())
        }
    }

    pub async fn set_refs(
        &self,
        refs: &[RefKey],
        db: &mut AsyncPgConnection,
    ) -> Result<(), Error> {
        for r in refs {
            let id = r.get_or_create_id(db).await?;
            diesel::insert_into(ar::article_refkeys)
                .values((ar::article_id.eq(self.id), ar::refkey_id.eq(id)))
                .on_conflict_do_nothing()
                .execute(db)
                .await?;
        }
        Ok(())
    }
}
