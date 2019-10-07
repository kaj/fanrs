use diesel::pg::Pg;
use diesel::serialize::{self, Output, ToSql};
use diesel::sql_types::Integer;
use diesel::Queryable;
use std::fmt::{Display, Formatter};
use std::io::Write;
use std::str::FromStr;

#[derive(Debug, PartialEq, Eq)]
pub struct Price {
    // Internal representation is a number of öre.
    price: i32,
}

impl Queryable<Integer, Pg> for Price {
    type Row = i32;
    fn build(price: i32) -> Price {
        Price { price }
    }
}

impl ToSql<Integer, Pg> for Price
where
    i32: ToSql<Integer, Pg>,
{
    fn to_sql<W: Write>(&self, out: &mut Output<W, Pg>) -> serialize::Result {
        ToSql::<Integer, Pg>::to_sql(&self.price, out)
    }
}

impl Display for Price {
    fn fmt(&self, out: &mut Formatter) -> std::fmt::Result {
        match (self.price / 100, self.price % 100) {
            (0, ore) => write!(out, "{} öre", ore),
            (kr, 0) => write!(out, "{}:-", kr),
            (kr, ore) => write!(out, "{}:{:02}", kr, ore),
        }
    }
}

impl FromStr for Price {
    type Err = <f32 as FromStr>::Err;
    fn from_str(input: &str) -> Result<Price, Self::Err> {
        let sek: f32 = input.parse()?;
        Ok(Price {
            price: (sek * 100.0).round() as i32,
        })
    }
}
