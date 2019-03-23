#![allow(proc_macro_derive_resolution_fallback)]
use crate::templates::ToHtml;
use std::io::{self, Write};

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

pub trait CloudItem: Ord {
    fn write_item(&self, out: &mut Write, n: i64, w: u8) -> io::Result<()>;
}

pub struct Cloud<T: CloudItem> {
    data: Vec<(T, i64, u8)>,
}

impl<T: CloudItem> Cloud<T> {
    fn from_ordered(data: Vec<(T, i64)>) -> Self {
        let num = data.len();
        let mut data = data
            .into_iter()
            .enumerate()
            .map(|(n, (title, c))| (title, c, (8 * (num - n) / num) as u8))
            .collect::<Vec<_>>();
        data.sort_by(|a, b| a.0.cmp(&b.0));
        Cloud { data }
    }
}

impl<T: CloudItem> ToHtml for Cloud<T> {
    fn to_html(&self, out: &mut Write) -> io::Result<()> {
        if let Some((last, titles)) = self.data.split_last() {
            for (item, n, w) in titles {
                item.write_item(out, *n, *w)?;
                write!(out, ", ")?;
            }
            let (item, n, w) = last;
            item.write_item(out, *n, *w)?;
            write!(out, ".")?;
        }
        Ok(())
    }
}
