extern crate actix_web;
extern crate http;
extern crate bytes;

use self::http::header;
use self::actix_web::{HttpRequest, Result, HttpResponse};
use self::actix_web::middleware::{Middleware, Response};
pub use self::actix_web::middleware::{Logger};

///Default headers middleware
pub struct DefaultHeaders;

impl<S> Middleware<S> for DefaultHeaders {
    fn response(&self, _: &mut HttpRequest<S>, mut resp: HttpResponse) -> Result<Response> {
        const DEFAULT_HEADERS: [(header::HeaderName, &'static str); 5] = [
            (header::SERVER, "Roseline"),
            (header::X_DNS_PREFETCH_CONTROL, "off"),
            (header::X_XSS_PROTECTION, "1; mode=block"),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
            (header::CONTENT_TYPE, "application/octet-stream"),
        ];

        {
            let headers = resp.headers_mut();
            for &(ref key, ref value) in DEFAULT_HEADERS.iter() {
                if !headers.contains_key(key) {
                    headers.insert(key, header::HeaderValue::from_static(value));
                }
            }
        }

        Ok(Response::Done(resp))
    }
}
