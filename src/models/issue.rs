use bigdecimal::BigDecimal;
use diesel;
use diesel::pg::PgConnection;
use diesel::prelude::*;
use diesel::result::Error;
use std::fmt;

#[derive(Debug, Queryable)]
pub struct Issue {
    pub id: i32,
    pub year: i16,
    pub number: i16,
    pub number_str: String,
    pub pages: Option<i16>,
    pub price: Option<BigDecimal>,
    pub cover_best: Option<i16>,
}

impl Issue {
    pub fn get_or_create(
        year: i16,
        number: i16,
        number_str: &str,
        pages: Option<i16>,
        price: Option<BigDecimal>,
        db: &PgConnection,
    ) -> Result<Issue, Error> {
        use schema::issues::dsl;
        if let Some(t) = dsl::issues
            .filter(dsl::year.eq(year))
            .filter(dsl::number.eq(number))
            .filter(dsl::number_str.eq(number_str))
            .first::<Issue>(db)
            .optional()?
        {
            Ok(t)
        } else {
            Ok(diesel::insert_into(dsl::issues)
                .values((
                    dsl::year.eq(year),
                    dsl::number.eq(number),
                    dsl::number_str.eq(number_str),
                    dsl::pages.eq(pages),
                    dsl::price.eq(price),
                ))
                .get_result(db)?)
        }
    }
    pub fn clear(&self, db: &PgConnection) -> Result<(), Error> {
        use schema::publications::dsl as p;
        diesel::delete(p::publications.filter(p::issue.eq(self.id)))
            .execute(db)?;
        Ok(())
    }
}

impl fmt::Display for Issue {
    fn fmt(&self, out: &mut fmt::Formatter) -> fmt::Result {
        write!(out, "{}/{}", self.number_str, self.year)?;
        match (&self.pages, &self.price) {
            (Some(ref pages), Some(ref price)) => {
                write!(out, " ({} sidor, {})", pages, price)
            }
            (Some(ref pages), None) => write!(out, " ({} sidor)", pages),
            (None, Some(ref price)) => write!(out, " ({})", price),
            (None, None) => Ok(()),
        }
    }
}
