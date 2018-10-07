#![allow(proc_macro_derive_resolution_fallback)]

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
    titles (id) {
        id -> Int4,
        title -> Varchar,
        slug -> Varchar,
    }
}

allow_tables_to_appear_in_same_query!(
    issues,
    titles,
);
