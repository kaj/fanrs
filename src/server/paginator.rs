use crate::templates::ToHtml;
use std::io::{self, Write};

#[derive(Debug)]
pub struct Paginator {
    n_pages: usize,
    page: usize,
}

const PAGE_SIZE: usize = 30;

impl Paginator {
    pub fn if_needed<T>(
        mut items: Vec<T>,
        page: Option<usize>,
    ) -> Result<(Vec<T>, Option<Paginator>), ()> {
        if items.len() / 3 > PAGE_SIZE {
            let n_pages = (items.len() - 1) / PAGE_SIZE + 1;
            let page = page.unwrap_or(1);
            if page < 1 || page > n_pages {
                return Err(());
            }
            items.drain(0..PAGE_SIZE * (page - 1));
            items.truncate(PAGE_SIZE);
            Ok((items, Some(Paginator { n_pages, page })))
        } else {
            Ok((items, None))
        }
    }
}

impl ToHtml for Paginator {
    fn to_html(&self, out: &mut dyn Write) -> io::Result<()> {
        fn one(out: &mut dyn Write, p: usize, pp: usize) -> io::Result<()> {
            if p == pp {
                write!(out, "<b>{}</b>", p)?;
            } else {
                write!(out, "<a href='?p={}'>{}</a>", p, p)?;
            }
            Ok(())
        }
        let from = if self.page > 7 { self.page - 5 } else { 1 };
        let to = if self.page + 7 < self.n_pages {
            self.page + 5
        } else {
            self.n_pages
        };
        if from > 1 {
            one(out, 1, self.page)?;
            write!(out, " … ")?;
        }
        one(out, from, self.page)?;
        for p in from + 1..=to {
            write!(out, ", ")?;
            one(out, p, self.page)?;
        }
        if to < self.n_pages {
            write!(out, " … ")?;
            one(out, self.n_pages, self.page)?;
        }
        Ok(())
    }
}
