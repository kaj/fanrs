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
    pub fn run(self) -> Result<(), Error> {
        let mut issue = self.issue;
        for pages in self.pages.windows(2) {
            // Irrefutable, because pages are 2-windows of a slice.
            if let [start, end] = pages {
                if end <= start {
                    return Err(format_err!("Page numbers must increase"));
                }
                println!("{:>6}: {:3}", issue, end - start);
                issue = (issue.last() + 1).to_string().parse()?;
            }
        }
        Ok(())
    }
}
