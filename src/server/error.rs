use std::fmt::{self, Display};
use tokio_diesel::AsyncError;
use warp::http::StatusCode;
use warp::reply::Response;
use warp::{Rejection, Reply};

#[derive(Debug, Clone)]
pub struct ServerError {
    code: StatusCode,
}

impl ServerError {
    pub fn not_found() -> Self {
        ServerError {
            code: StatusCode::NOT_FOUND,
        }
    }
}

impl Reply for ServerError {
    fn into_response(self) -> Response {
        use super::templates::{error, notfound, RenderRucte};
        use warp::http::response::Builder;
        log::error!("{}", self);
        let res = Builder::new().status(self.code);
        if self.code == StatusCode::NOT_FOUND {
            res.html(|o| notfound(o, StatusCode::NOT_FOUND)).unwrap()
        } else {
            res.html(|o| error(o, self.code)).unwrap()
        }
    }
}
impl std::error::Error for ServerError {}

impl Display for ServerError {
    fn fmt(&self, out: &mut fmt::Formatter) -> fmt::Result {
        write!(out, "Error: {}", self.code)
    }
}

impl From<Rejection> for ServerError {
    fn from(err: Rejection) -> ServerError {
        log::error!("Reject {:?}", err);
        let code = if err.is_not_found() {
            StatusCode::NOT_FOUND
        } else if let Some(_) = err.find::<warp::reject::MethodNotAllowed>() {
            StatusCode::METHOD_NOT_ALLOWED
        } else {
            StatusCode::INTERNAL_SERVER_ERROR
        };
        ServerError { code }
    }
}

impl From<AsyncError> for ServerError {
    fn from(err: AsyncError) -> ServerError {
        let code = match err {
            AsyncError::Checkout(e) => {
                log::error!("Pool error: {}", e);
                StatusCode::SERVICE_UNAVAILABLE
            }
            AsyncError::Error(e) => {
                log::error!("Error: {}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            }
        };
        ServerError { code }
    }
}
/*impl From<warp::http::Error> for ServerError {
    fn from(err: warp::http::Error) -> ServerError {
        ServerError {
            code: err.into(),
        }
    }
}*/

pub trait OptionalExtension<T>: tokio_diesel::OptionalExtension<T> {
    fn or_404(self) -> Result<T, ServerError>
    where
        Self: Sized,
    {
        match tokio_diesel::OptionalExtension::optional(self)? {
            Some(t) => Ok(t),
            None => Err(ServerError::not_found()),
        }
    }
    fn optional(self) -> Result<Option<T>, ServerError>
    where
        Self: Sized,
    {
        Ok(tokio_diesel::OptionalExtension::optional(self)?)
    }
}

impl<T> OptionalExtension<T> for Result<T, AsyncError> {}
