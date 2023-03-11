use diesel::backend::Backend;
use diesel::deserialize::{self, FromSql, FromSqlRow};
use diesel::pg::Pg;
use diesel::serialize::{self, Output, ToSql};
use diesel::sql_types::Integer;
use std::fmt::{Display, Formatter};
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
    type Err = <f32 as FromStr>::Err;
    fn from_str(input: &str) -> Result<Price, Self::Err> {
        let sek: f32 = input.parse()?;
        Ok(Price {
            price: (sek * 100.0).round() as i32,
        })
    }
}
