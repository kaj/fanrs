use crate::templates::{error_html, notfound_html, RenderError, RenderRucte};
use diesel_async::pooled_connection::deadpool::PoolError;
use log::error;
use warp::http::response::Builder;
use warp::http::status::StatusCode;
use warp::reply::Response;
use warp::{self, Rejection, Reply};

#[derive(Debug)]
pub enum ViewError {
    /// 404
    NotFound,
    /// 503
    ServiceUnavailable,
    /// 500
    Err(&'static str),
}

pub trait ViewResult<T> {
    fn ise(self) -> Result<T, ViewError>;
}

impl<T, E> ViewResult<T> for Result<T, E>
where
    E: std::error::Error,
{
    fn ise(self) -> Result<T, ViewError> {
        self.map_err(|e| {
            error!("Internal server error: {:?}", e);
            ViewError::Err("Något gick snett")
        })
    }
}

impl Reply for ViewError {
    fn into_response(self) -> Response {
        match self {
            ViewError::NotFound => {
                let code = StatusCode::NOT_FOUND;
                Builder::new()
                    .status(code)
                    .html(|o| notfound_html(o, code))
                    .unwrap()
            }
            ViewError::ServiceUnavailable => error_response(
                StatusCode::SERVICE_UNAVAILABLE,
                "Servern är överlastad",
                "Fantomen vilar först då fred råder i världen. \
                 Den här webbservern verkar dock behöva lite vila just nu. \
                 Försök gärna igen om ett litet tag.",
            ),
            ViewError::Err(msg) => error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                msg,
                "Något gick snett. \
                 Detta missöde är noterat i webserverns krönika. \
                 Du kanske kan försöka igen om en stund? \
                 Eller gå tillbaks till \
                 <a href='/'>fantomenindexets förstasida</a>? \
                 Om det fortfarande verkar trasigt så får du gärna rapportera \
                 felet till \
                 <a href='mailto:rasmus@krats.se'>rasmus@krats.se</a>.",
            ),
        }
    }
}

fn error_response(code: StatusCode, message: &str, detail: &str) -> Response {
    Builder::new()
        .status(code)
        .html(|o| error_html(o, code, message, detail))
        .unwrap()
}

impl From<RenderError> for ViewError {
    fn from(e: RenderError) -> Self {
        error!("Rendering error: {}\n    {:?}", e, e);
        ViewError::Err("Renderingsfel")
    }
}

impl From<diesel::result::Error> for ViewError {
    fn from(e: diesel::result::Error) -> Self {
        error!("Database error: {}\n    {:?}", e, e);
        ViewError::Err("Databasfel")
    }
}

impl From<PoolError> for ViewError {
    fn from(e: PoolError) -> Self {
        match e {
            PoolError::Timeout(kind) => {
                error!("Db Pool timeout: {:?}", kind);
                ViewError::ServiceUnavailable
            }
            e => {
                error!("Db Pool error: {:?}", e);
                ViewError::Err("Databasfel")
            }
        }
    }
}

/// Create custom errors for warp rejections.
///
/// Currently only handles 404, as there is no way of getting any
/// details out of the other build-in rejections in warp.
pub async fn for_rejection(err: Rejection) -> Result<Response, Rejection> {
    if err.is_not_found() {
        Ok(ViewError::NotFound.into_response())
    } else {
        Err(err)
    }
}
