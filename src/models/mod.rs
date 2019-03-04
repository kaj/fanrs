#![allow(proc_macro_derive_resolution_fallback)]

mod article;
mod creator;
mod creatorset;
mod episode;
mod issue;
mod other_mag;
mod part;
mod refkey;
mod refkeyset;
mod title;

pub use self::article::Article;
pub use self::creator::Creator;
pub use self::creatorset::CreatorSet;
pub use self::episode::Episode;
pub use self::issue::{Issue, IssueRef, Nr};
pub use self::other_mag::OtherMag;
pub use self::part::{Part, PartInIssue};
pub use self::refkey::{IdRefKey, RefKey};
pub use self::refkeyset::RefKeySet;
pub use self::title::Title;
