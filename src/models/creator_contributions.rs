use super::{Creator, IssueRef};
use diesel::Queryable;

#[derive(Queryable)]
pub struct CreatorContributions {
    pub creator: Creator,
    pub score: i32,
    pub n_episodes: i32,
    pub n_covers: i32,
    pub n_articles: i32,
    pub first_issue: Option<IssueRef>,
    pub latest_issue: Option<IssueRef>,
}

diesel::table! {
    /// This is a materialzied view, and apparently not included in
    /// diesel schema generation.
    creator_contributions (id) {
        id -> Int4,
        name -> Varchar,
        slug -> Varchar,
        score -> Int4,
        n_episodes -> Int4,
        n_covers -> Int4,
        n_articles -> Int4,
        first_issue -> Nullable<Int2>,
        latest_issue -> Nullable<Int2>,
    }
}
