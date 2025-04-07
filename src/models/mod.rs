#![allow(proc_macro_derive_resolution_fallback)]
use crate::templates::ToHtml;
use std::fmt;
use std::io::{self, Write};

mod article;
mod creator;
pub mod creator_contributions;
mod creatorset;
mod episode;
mod issue;
mod other_mag;
mod part;
mod price;
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
    fn write_item(
        &self,
        out: &mut dyn Write,
        n: i32,
        w: u8,
    ) -> io::Result<()>;
}

pub struct Cloud<T: CloudItem> {
    data: Vec<(T, i32, u8)>,
}

impl<T: CloudItem> Cloud<T> {
    fn from_ordered(data: Vec<(T, i32)>) -> Self {
        let num = data.len();
        let w = |n| u8::try_from(17 * (num - n) / num / 2).unwrap_or(u8::MAX);
        let mut data = data
            .into_iter()
            .enumerate()
            .map(|(n, (item, c))| (item, c, w(n)))
            .collect::<Vec<_>>();
        data.sort_by(|a, b| a.0.cmp(&b.0));
        Cloud { data }
    }
}

impl<T: CloudItem> ToHtml for Cloud<T> {
    fn to_html(&self, out: &mut dyn Write) -> io::Result<()> {
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

pub struct YearNo(i16);
impl YearNo {
    pub fn of(year: i16) -> Self {
        Self(year + 1 - 1950)
    }
}
impl fmt::Display for YearNo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let n = self.0;
        n.fmt(f)?;
        if n == 1 || n == 2 || (n > 20 && (n % 10 == 1 || n % 10 == 2)) {
            f.write_str(":a")?;
        } else {
            f.write_str(":e")?;
        }
        Ok(())
    }
}
