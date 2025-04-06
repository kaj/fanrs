// @generated automatically by Diesel CLI.

diesel::table! {
    article_refkeys (id) {
        id -> Int4,
        article_id -> Int4,
        refkey_id -> Int4,
    }
}

diesel::table! {
    articles (id) {
        id -> Int4,
        #[max_length = 200]
        title -> Varchar,
        #[max_length = 500]
        subtitle -> Nullable<Varchar>,
        note -> Nullable<Text>,
    }
}

diesel::table! {
    articles_by (id) {
        id -> Int4,
        article_id -> Int4,
        creator_alias_id -> Int4,
        #[max_length = 10]
        role -> Varchar,
    }
}

diesel::table! {
    covers (id) {
        id -> Int4,
        issue -> Int4,
        image -> Bytea,
        fetch_time -> Timestamp,
    }
}

diesel::table! {
    covers_by (id) {
        id -> Int4,
        issue_id -> Int4,
        creator_alias_id -> Int4,
    }
}

diesel::table! {
    creator_aliases (id) {
        id -> Int4,
        creator_id -> Int4,
        #[max_length = 200]
        name -> Varchar,
    }
}

diesel::table! {
    creators (id) {
        id -> Int4,
        #[max_length = 200]
        name -> Varchar,
        #[max_length = 200]
        slug -> Varchar,
    }
}

diesel::table! {
    episode_parts (id) {
        id -> Int4,
        episode_id -> Int4,
        part_no -> Nullable<Int2>,
        #[max_length = 200]
        part_name -> Nullable<Varchar>,
    }
}

diesel::table! {
    episode_refkeys (id) {
        id -> Int4,
        episode_id -> Int4,
        refkey_id -> Int4,
    }
}

diesel::table! {
    episodes (id) {
        id -> Int4,
        title_id -> Int4,
        name -> Nullable<Varchar>,
        teaser -> Nullable<Varchar>,
        note -> Nullable<Varchar>,
        copyright -> Nullable<Varchar>,
        orig_lang -> Nullable<Varchar>,
        orig_episode -> Nullable<Varchar>,
        orig_date -> Nullable<Date>,
        orig_to_date -> Nullable<Date>,
        orig_sundays -> Bool,
        orig_mag_id -> Nullable<Int4>,
        strip_from -> Nullable<Int4>,
        strip_to -> Nullable<Int4>,
    }
}

diesel::table! {
    episodes_by (id) {
        id -> Int4,
        episode_id -> Int4,
        creator_alias_id -> Int4,
        #[max_length = 10]
        role -> Varchar,
    }
}

diesel::table! {
    issues (id) {
        id -> Int4,
        year -> Int2,
        number -> Int2,
        #[max_length = 6]
        number_str -> Varchar,
        pages -> Nullable<Int2>,
        price -> Nullable<Int4>,
        cover_best -> Nullable<Int2>,
        magic -> Int2,
        ord -> Nullable<Int4>,
    }
}

diesel::table! {
    other_mags (id) {
        id -> Int4,
        name -> Varchar,
        issue -> Nullable<Int2>,
        i_of -> Nullable<Int2>,
        year -> Nullable<Int2>,
    }
}

diesel::table! {
    publications (id) {
        id -> Int4,
        issue_id -> Int4,
        seqno -> Nullable<Int2>,
        episode_part -> Nullable<Int4>,
        best_plac -> Nullable<Int2>,
        article_id -> Nullable<Int4>,
        label -> Varchar,
    }
}

diesel::table! {
    refkeys (id) {
        id -> Int4,
        kind -> Int2,
        title -> Varchar,
        #[max_length = 100]
        slug -> Varchar,
    }
}

diesel::table! {
    titles (id) {
        id -> Int4,
        title -> Varchar,
        slug -> Varchar,
    }
}

diesel::joinable!(article_refkeys -> articles (article_id));
diesel::joinable!(article_refkeys -> refkeys (refkey_id));
diesel::joinable!(articles_by -> articles (article_id));
diesel::joinable!(articles_by -> creator_aliases (creator_alias_id));
diesel::joinable!(covers -> issues (issue));
diesel::joinable!(covers_by -> creator_aliases (creator_alias_id));
diesel::joinable!(covers_by -> issues (issue_id));
diesel::joinable!(creator_aliases -> creators (creator_id));
diesel::joinable!(episode_parts -> episodes (episode_id));
diesel::joinable!(episode_refkeys -> episodes (episode_id));
diesel::joinable!(episode_refkeys -> refkeys (refkey_id));
diesel::joinable!(episodes -> other_mags (orig_mag_id));
diesel::joinable!(episodes -> titles (title_id));
diesel::joinable!(episodes_by -> creator_aliases (creator_alias_id));
diesel::joinable!(episodes_by -> episodes (episode_id));
diesel::joinable!(publications -> articles (article_id));
diesel::joinable!(publications -> episode_parts (episode_part));
diesel::joinable!(publications -> issues (issue_id));

diesel::allow_tables_to_appear_in_same_query!(
    article_refkeys,
    articles,
    articles_by,
    covers,
    covers_by,
    creator_aliases,
    creators,
    episode_parts,
    episode_refkeys,
    episodes,
    episodes_by,
    issues,
    other_mags,
    publications,
    refkeys,
    titles,
);
