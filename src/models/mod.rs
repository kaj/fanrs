#![allow(proc_macro_derive_resolution_fallback)]

mod article;
mod creator;
mod creatorset;
mod episode;
mod issue;
mod part;
mod refkey;
mod refkeyset;
mod title;

pub use self::article::Article;
pub use self::creator::Creator;
pub use self::creatorset::CreatorSet;
pub use self::episode::Episode;
pub use self::issue::{parse_nr, Issue, IssueRef};
pub use self::part::{Part, PartInIssue};
pub use self::refkey::RefKey;
pub use self::refkeyset::RefKeySet;
pub use self::title::Title;
