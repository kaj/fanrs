use super::{PgPool, Result, ViewError, redirect};
use crate::schema::covers::dsl as c;
use crate::schema::issues::dsl as i;
use crate::templates::statics::xcover_jpg;
use chrono::{Duration, Utc};
use diesel::OptionalExtension;
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use mime::IMAGE_JPEG;
use std::num::ParseIntError;
use std::str::FromStr;
use warp::Reply;
use warp::http::header::{CONTENT_TYPE, EXPIRES};
use warp::http::{Response, StatusCode};

#[allow(clippy::needless_pass_by_value)]
pub async fn cover_image(issue: CoverRef, db: PgPool) -> Result<impl Reply> {
    let data = i::issues
        .inner_join(c::covers)
        .select(c::image)
        .filter(i::year.eq(issue.year))
        .filter(i::number.eq(issue.number))
        .first::<Vec<u8>>(&mut db.get().await?)
        .await
        .optional()?;

    if let Some(data) = data {
        let medium_expires = Utc::now() + Duration::days(90);
        Ok(Response::builder()
            .header(CONTENT_TYPE, IMAGE_JPEG.as_ref())
            .header(EXPIRES, medium_expires.to_rfc2822())
            .body(data))
    } else {
        Ok(Response::builder()
            .status(StatusCode::NOT_FOUND)
            .header(CONTENT_TYPE, xcover_jpg.mime.as_ref())
            .body(xcover_jpg.content.to_vec()))
    }
}

pub struct CoverRef {
    year: i16,
    number: i16,
}

impl FromStr for CoverRef {
    type Err = u8;
    /// expect fYYYY-NN.jpg
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if !s.starts_with('f') {
            return Err(0);
        }
        let p1 = s.find('-').ok_or(1)?;
        let p2 = s.find(".jpg").ok_or(2)?;
        Ok(CoverRef {
            year: s[1..p1].parse().map_err(|_| 3)?,
            number: s[p1 + 1..p2].parse().map_err(|_| 4)?,
        })
    }
}

pub async fn redirect_cover(
    year: CYear,
    issue: SIssue,
    db: PgPool,
) -> Result<impl Reply> {
    let mut db = db.get().await?;
    let exists = i::issues
        .filter(i::year.eq(year.0))
        .filter(i::number.eq(issue.0))
        .count()
        .get_result::<i64>(&mut db)
        .await?;
    if exists > 0 {
        redirect(&format!("/c/f{}-{}.jpg", year.0, issue.0))
    } else {
        Err(ViewError::NotFound)
    }
}

pub struct CYear(i16);

impl FromStr for CYear {
    type Err = u8;
    /// expect cYYYY
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if !s.starts_with('c') {
            return Err(0);
        }
        Ok(CYear(s[1..].parse().map_err(|_| 3)?))
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct SIssue(i16);

impl FromStr for SIssue {
    type Err = SIssueError;
    /// expect sNN.jpg or sN-M.jpg where M = N + 1
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.strip_prefix('s').ok_or(SIssueError::BadPrefix)?;
        let s = s.strip_suffix(".jpg").ok_or(SIssueError::BadSuffix)?;

        if let Some(p) = s.find('-') {
            let n = s[..p].parse()?;
            let m = s[p + 1..].parse()?;
            if m == n + 1 {
                Ok(SIssue(n))
            } else {
                Err(SIssueError::Double(n, m))
            }
        } else {
            Ok(SIssue(s.parse()?))
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SIssueError {
    #[error("Image name should start with 's'")]
    BadPrefix,
    #[error("Image name should end with '.jpg'")]
    BadSuffix,
    #[error("Bad image name: {0}")]
    BadNr(#[from] ParseIntError),
    #[error("Strange double nr {0}-{1}")]
    Double(i16, i16),
}

#[cfg(test)]
mod test {
    use super::SIssue;

    #[test]
    fn simple() {
        assert_eq!("s3.jpg".parse(), Ok(SIssue(3)));
    }

    #[test]
    fn double() {
        assert_eq!("s7-8.jpg".parse(), Ok(SIssue(7)));
    }

    #[test]
    fn bad() {
        for name in ["p3.jpg", "s3.jpeg", "s.jpg", "sOne.jpg", "s1-4.jpg"] {
            assert!(
                name.parse::<SIssue>().is_err(),
                "Expected {name:?} to fail"
            );
        }
    }
}
