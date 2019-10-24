use crate::models::Nr;
use structopt::StructOpt;

#[derive(StructOpt)]
pub struct CountPages {
    /// Issue followed by page number where it and following issues
    /// are starting.
    // #[structopt(long, short)]
    issue: Nr,
    pages: Vec<u16>,
}

impl CountPages {
    pub fn run(&self) {
        let mut issue = self.issue.clone();
        let mut page = self.pages.iter();
        let mut prev = page.next().unwrap();
        while let Some(page) = page.next() {
            println!("{:>6}: {:3}", issue, page - prev);
            prev = page;
            issue = (issue.last() + 1).to_string().parse().unwrap();
        }
    }
}
