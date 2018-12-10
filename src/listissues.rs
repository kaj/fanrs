use crate::schema::issues::dsl;
use diesel::pg::PgConnection;
use diesel::prelude::*;
use failure::Error;
use std::collections::BTreeMap;

pub fn list_issues(db: &PgConnection) -> Result<(), Error> {
    let mut all: BTreeMap<i16, Vec<i16>> = BTreeMap::new();
    for (year, number) in dsl::issues
        .select((dsl::year, dsl::number))
        .order((dsl::year, dsl::number))
        .load::<(i16, i16)>(db)?
    {
        all.entry(year).or_default().push(number);
    }
    for (year, numbers) in &all {
        print!("{}: ", year);
        let mut iter = numbers.iter().peekable();
        while let Some(n) = iter.next() {
            let mut end = n;
            while iter.peek() == Some(&&(end + 1)) {
                end = iter.next().unwrap();
            }
            if end > n {
                print!("{} - {}", n, end);
            } else {
                print!("{}", n);
            }
            if iter.peek().is_some() {
                print!(", ");
            }
        }
        println!()
    }
    Ok(())
}
