use diesel::backend::Backend;
use diesel::deserialize::{self, FromSql, FromSqlRow};
use diesel::pg::Pg;
use diesel::serialize::{self, Output, ToSql};
use diesel::sql_types::Integer;
use std::fmt::{Display, Formatter};
use std::num::ParseIntError;
use std::str::FromStr;

#[derive(Debug, FromSqlRow, PartialEq, Eq)]
pub struct Price {
    // Internal representation is a number of öre.
    price: i32,
}

impl FromSql<Integer, Pg> for Price {
    fn from_sql(
        bytes: <Pg as Backend>::RawValue<'_>,
    ) -> deserialize::Result<Self> {
        FromSql::<Integer, Pg>::from_sql(bytes).map(|price| Price { price })
    }
}

impl ToSql<Integer, Pg> for Price {
    fn to_sql<'a>(
        &'a self,
        out: &mut Output<'a, '_, Pg>,
    ) -> serialize::Result {
        ToSql::<Integer, Pg>::to_sql(&self.price, out)
    }
}

impl Display for Price {
    fn fmt(&self, out: &mut Formatter) -> std::fmt::Result {
        match (self.price / 100, self.price % 100) {
            (0, ore) => write!(out, "{ore} öre"),
            (kr, 0) => write!(out, "{kr}:-"),
            (kr, ore) => write!(out, "{kr}:{ore:02}"),
        }
    }
}

impl FromStr for Price {
    type Err = BadPrice;
    fn from_str(input: &str) -> Result<Price, Self::Err> {
        let (sek, dec) = input.split_once('.').unwrap_or((input, ""));
        let sek = if sek.is_empty() {
            0
        } else {
            sek.parse::<i32>()?.checked_mul(100).ok_or(BadPrice)?
        };
        let dec = match dec.len() {
            0 => 0,
            2 => dec.parse::<i32>()?,
            _ => return Err(BadPrice),
        };
        let price = sek.checked_add(dec).ok_or(BadPrice)?;
        Ok(Price { price })
    }
}

#[derive(Debug)]
pub struct BadPrice;
impl std::error::Error for BadPrice {}
impl Display for BadPrice {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str("bad price")
    }
}
impl From<ParseIntError> for BadPrice {
    fn from(_: ParseIntError) -> Self {
        BadPrice
    }
}
