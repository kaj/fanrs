use crate::models::Nr;
use crate::schema::issues::dsl as i;
use crate::schema::publications::dsl as p;
use anyhow::Result;
use diesel::pg::PgConnection;
use diesel::prelude::*;
use std::collections::BTreeMap;

pub fn list_issues(db: &PgConnection) -> Result<()> {
    let mut all = BTreeMap::<i16, Vec<Nr>>::new();
    for (year, number) in i::issues
        .select((i::year, (i::number, i::number_str)))
        .inner_join(p::publications)
        .filter(p::seqno.is_not_null())
        .group_by((i::year, (i::number, i::number_str)))
        .order((i::year, i::number))
        .load(db)?
    {
        all.entry(year).or_default().push(number);
    }

    println!(
        "# Indexerade tidningar ({:?} stycken)",
        all.values().map(|issues| issues.len()).sum::<usize>()
    );
    println!();

    let mut decade = 0;
    for (year, numbers) in &all {
        if year / 10 != decade {
            decade = year / 10;
            println!("{decade}0");
        }
        print!("- {year}: ");
        let mut iter = numbers.iter().peekable();
        while let Some(n) = iter.next() {
            let mut end = n;
            while iter.peek().map(|n| n.first()) == Some(end.last() + 1) {
                end = iter.next().unwrap();
            }
            if end > n {
                print!("{n} - {end}");
            } else {
                print!("{n}");
            }
            if iter.peek().is_some() {
                print!(", ");
            }
        }
        println!()
    }
    Ok(())
}
