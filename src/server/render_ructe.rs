/// This module defines the `RenderRucte` trait for a response builer.
///
/// If ructe gets a warp feature, this is probably it.
use mime::TEXT_HTML_UTF_8;
use std::io::{self, Write};
use warp::http::header::CONTENT_TYPE;
use warp::http::response::Builder;
use warp::http::Response;
use warp::{reject::custom, Rejection};

pub trait RenderRucte {
    fn html<F>(&mut self, f: F) -> Result<Response<Vec<u8>>, Rejection>
    where
        F: FnOnce(&mut Write) -> io::Result<()>;
}

impl RenderRucte for Builder {
    fn html<F>(&mut self, f: F) -> Result<Response<Vec<u8>>, Rejection>
    where
        F: FnOnce(&mut Write) -> io::Result<()>,
    {
        let mut buf = Vec::new();
        f(&mut buf).map_err(custom)?;
        self.header(CONTENT_TYPE, TEXT_HTML_UTF_8.as_ref())
            .body(buf)
            .map_err(custom)
    }
}
