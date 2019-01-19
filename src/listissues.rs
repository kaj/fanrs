use crate::models::Nr;
use crate::schema::issues::dsl as i;
use diesel::pg::PgConnection;
use diesel::prelude::*;
use failure::Error;
use std::collections::BTreeMap;

pub fn list_issues(db: &PgConnection) -> Result<(), Error> {
    let mut all = BTreeMap::<i16, Vec<Nr>>::new();
    for (year, number) in i::issues
        .select((i::year, (i::number, i::number_str)))
        .order((i::year, i::number))
        .load(db)?
    {
        all.entry(year).or_default().push(number);
    }
    for (year, numbers) in &all {
        print!("{}: ", year);
        let mut iter = numbers.iter().peekable();
        while let Some(n) = iter.next() {
            let mut end = n;
            while iter.peek().map(|n| n.first()) == Some(end.last() + 1) {
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
