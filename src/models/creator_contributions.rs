use super::{Creator, IssueRef};

#[derive(Queryable)]
pub struct CreatorContributions {
    pub creator: Creator,
    pub n_episodes: i64,
    pub n_covers: i64,
    pub n_articles: i64,
    pub first_issue: Option<IssueRef>,
    pub latest_issue: Option<IssueRef>,
}

table! {
    /// This is a materialzied view, and apparently not included in
    /// diesel schema generation.
    creator_contributions (id) {
        id -> Int4,
        name -> Varchar,
        slug -> Varchar,
        n_episodes -> Int8,
        n_covers -> Int8,
        n_articles -> Int8,
        first_issue -> Nullable<Int2>,
        latest_issue -> Nullable<Int2>,
    }
}
