use diesel::pg::PgConnection;
use diesel::prelude::*;
use failure::Error;
use schema::issues::dsl;
use std::collections::BTreeMap;

pub fn list_issues(db: &PgConnection) -> Result<(), Error> {
    let mut all = BTreeMap::new();
    for (year, number) in dsl::issues
        .select((dsl::year, dsl::number))
        .order((dsl::year, dsl::number))
        .load::<(i16, i16)>(db)?
    {
        all.entry(year).or_insert(vec![]).push(number);
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
