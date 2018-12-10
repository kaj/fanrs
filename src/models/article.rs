use super::RefKey;
use diesel;
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
        use crate::schema::articles::dsl;
        if let Some(article) = dsl::articles
            .filter(dsl::title.eq(title))
            .filter(dsl::subtitle.eq(subtitle))
            .filter(dsl::note.eq(note))
            .first::<Article>(db)
            .optional()?
        {
            Ok(article)
        } else {
            diesel::insert_into(dsl::articles)
                .values((
                    dsl::title.eq(title),
                    dsl::subtitle.eq(subtitle),
                    dsl::note.eq(note),
                ))
                .get_result(db)
        }
    }

    /// This article is published in a specific issue.
    pub fn publish(
        &self,
        issue: i32,
        seqno: Option<i16>,
        db: &PgConnection,
    ) -> Result<(), Error> {
        use crate::schema::publications::dsl as p;
        if let Some((id, old_seqno)) = p::publications
            .filter(p::issue.eq(issue))
            .filter(p::article_id.eq(self.id))
            .select((p::id, p::seqno))
            .first::<(i32, Option<i16>)>(db)
            .optional()?
        {
            if seqno.is_some() && old_seqno != seqno {
                eprintln!("TODO: Should update seqno for {}", id);
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
            use crate::schema::article_refkeys::dsl as ar;
            let id = r.get_or_create_id(db)?;
            diesel::insert_into(ar::article_refkeys)
                .values((ar::article_id.eq(self.id), ar::refkey_id.eq(id)))
                .on_conflict_do_nothing()
                .execute(db)?;
        }
        Ok(())
    }
}
