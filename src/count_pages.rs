use crate::models::Nr;
use failure::{format_err, Error};
use structopt::StructOpt;

#[derive(StructOpt)]
pub struct CountPages {
    /// The issue starting at the first given page number.
    issue: Nr,
    /// Page numbers where new issues start.
    pages: Vec<u16>,
}

impl CountPages {
    pub fn run(&self) -> Result<(), Error> {
        let mut issue = self.issue.clone();
        let mut page = self.pages.iter();
        let mut prev = page.next().ok_or(format_err!("Too few arguments"))?;
        while let Some(page) = page.next() {
            if page <= prev {
                return Err(format_err!("Page numbers must increase"));
            }
            println!("{:>6}: {:3}", issue, page - prev);
            prev = page;
            issue = (issue.last() + 1).to_string().parse()?;
        }
        Ok(())
    }
}
