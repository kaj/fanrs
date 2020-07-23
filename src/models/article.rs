use super::RefKey;
use crate::schema::article_refkeys::dsl as ar;
use crate::schema::articles::dsl as a;
use crate::schema::publications::dsl as p;
use diesel::pg::PgConnection;
use diesel::prelude::*;
use diesel::result::Error;

#[derive(Debug, Queryable)]
pub struct Article {
    pub id: i32,
    pub title: String,
    pub subtitle: Option<String>,
    pub note: Option<String>,
}

impl Article {
    pub fn get_or_create(
        title: &str,
        subtitle: Option<&str>,
        note: Option<&str>,
        db: &PgConnection,
    ) -> Result<Article, Error> {
        if let Some(article) = a::articles
            .filter(a::title.eq(title))
            .filter(a::subtitle.is_not_distinct_from(subtitle))
            .filter(a::note.is_not_distinct_from(note))
            .first::<Article>(db)
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
                .get_result(db)?)
        }
    }

    /// This article is published in a specific issue.
    pub fn publish(
        &self,
        issue: i32,
        seqno: i16,
        db: &PgConnection,
    ) -> Result<(), Error> {
        if let Some((id, old_seqno)) = p::publications
            .filter(p::issue.eq(issue))
            .filter(p::article_id.eq(self.id))
            .select((p::id, p::seqno))
            .first::<(i32, Option<i16>)>(db)
            .optional()?
        {
            if old_seqno != Some(seqno) {
                log::warn!(
                    "TODO: Should update seqno for article #{} ({:?} != {})",
                    id,
                    old_seqno,
                    seqno
                );
            }
            Ok(())
        } else {
            diesel::insert_into(p::publications)
                .values((
                    p::issue.eq(issue),
                    p::article_id.eq(self.id),
                    p::seqno.eq(seqno),
                ))
                .execute(db)?;
            Ok(())
        }
    }

    pub fn set_refs(
        &self,
        refs: &[RefKey],
        db: &PgConnection,
    ) -> Result<(), Error> {
        for r in refs {
            let id = r.get_or_create_id(db)?;
            diesel::insert_into(ar::article_refkeys)
                .values((ar::article_id.eq(self.id), ar::refkey_id.eq(id)))
                .on_conflict_do_nothing()
                .execute(db)?;
        }
        Ok(())
    }
}
