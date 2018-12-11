#![allow(proc_macro_derive_resolution_fallback)]

table! {
    article_refkeys (id) {
        id -> Int4,
        article_id -> Int4,
        refkey_id -> Int4,
    }
}

table! {
    articles (id) {
        id -> Int4,
        title -> Varchar,
        subtitle -> Nullable<Varchar>,
        note -> Nullable<Text>,
    }
}

table! {
    articles_by (id) {
        id -> Int4,
        article_id -> Int4,
        by_id -> Int4,
        role -> Varchar,
    }
}

table! {
    covers_by (id) {
        id -> Int4,
        issue_id -> Int4,
        by_id -> Int4,
    }
}

table! {
    creator_aliases (id) {
        id -> Int4,
        creator_id -> Int4,
        name -> Varchar,
    }
}

table! {
    creators (id) {
        id -> Int4,
        name -> Varchar,
        slug -> Varchar,
    }
}

table! {
    episode_parts (id) {
        id -> Int4,
        episode -> Int4,
        part_no -> Nullable<Int2>,
        part_name -> Nullable<Varchar>,
    }
}

table! {
    episode_refkeys (id) {
        id -> Int4,
        episode_id -> Int4,
        refkey_id -> Int4,
    }
}

table! {
    episodes (id) {
        id -> Int4,
        title -> Int4,
        episode -> Nullable<Varchar>,
        teaser -> Nullable<Varchar>,
        note -> Nullable<Varchar>,
        copyright -> Nullable<Varchar>,
    }
}

table! {
    episodes_by (id) {
        id -> Int4,
        episode_id -> Int4,
        by_id -> Int4,
        role -> Varchar,
    }
}

table! {
    issues (id) {
        id -> Int4,
        year -> Int2,
        number -> Int2,
        number_str -> Varchar,
        pages -> Nullable<Int2>,
        price -> Nullable<Numeric>,
        cover_best -> Nullable<Int2>,
    }
}

table! {
    publications (id) {
        id -> Int4,
        issue -> Int4,
        seqno -> Nullable<Int2>,
        episode_part -> Nullable<Int4>,
        best_plac -> Nullable<Int2>,
        article_id -> Nullable<Int4>,
    }
}

table! {
    refkeys (id) {
        id -> Int4,
        kind -> Int2,
        title -> Nullable<Varchar>,
        slug -> Varchar,
    }
}

table! {
    titles (id) {
        id -> Int4,
        title -> Varchar,
        slug -> Varchar,
    }
}

joinable!(article_refkeys -> articles (article_id));
joinable!(article_refkeys -> refkeys (refkey_id));
joinable!(articles_by -> articles (article_id));
joinable!(articles_by -> creator_aliases (by_id));
joinable!(covers_by -> creator_aliases (by_id));
joinable!(covers_by -> issues (issue_id));
joinable!(creator_aliases -> creators (creator_id));
joinable!(episode_parts -> episodes (episode));
joinable!(episode_refkeys -> episodes (episode_id));
joinable!(episode_refkeys -> refkeys (refkey_id));
joinable!(episodes -> titles (title));
joinable!(episodes_by -> creator_aliases (by_id));
joinable!(episodes_by -> episodes (episode_id));
joinable!(publications -> articles (article_id));
joinable!(publications -> episode_parts (episode_part));
joinable!(publications -> issues (issue));

allow_tables_to_appear_in_same_query!(
    article_refkeys,
    articles,
    articles_by,
    covers_by,
    creator_aliases,
    creators,
    episode_parts,
    episode_refkeys,
    episodes,
    episodes_by,
    issues,
    publications,
    refkeys,
    titles,
);
