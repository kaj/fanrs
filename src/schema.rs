#![allow(proc_macro_derive_resolution_fallback)]

table! {
    episode_parts (id) {
        id -> Int4,
        episode -> Int4,
        part_no -> Nullable<Int2>,
        part_name -> Nullable<Varchar>,
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
    }
}

table! {
    titles (id) {
        id -> Int4,
        title -> Varchar,
        slug -> Varchar,
    }
}

joinable!(episode_parts -> episodes (episode));
joinable!(episodes -> titles (title));
joinable!(publications -> episode_parts (episode_part));
joinable!(publications -> issues (issue));

allow_tables_to_appear_in_same_query!(
    episode_parts,
    episodes,
    issues,
    publications,
    titles,
);
