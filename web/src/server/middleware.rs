extern crate actix_web;
extern crate http;
extern crate bytes;

use self::http::header;
use self::actix_web::{HttpRequest, Result, HttpResponse};
use self::actix_web::middleware::{Middleware, Response, Started, Finished};

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

///Debug only logger.
///
///Prints to info category but disabled unless `debug_assertions` are on
pub struct Logger;

impl<S> Middleware<S> for Logger {
    #[allow(unused)]
    fn start(&self, req: &mut HttpRequest<S>) -> Result<Started> {
        #[cfg(debug_assertions)]
        {
            use self::actix_web::HttpMessage;
            use ::std::str;

            info!("HTTP: {remote} --> {method} {path} {version:?}\n{headers}\n",
                  remote=req.connection_info().remote().unwrap_or(""),
                  method=req.method(),
                  path=req.path(),
                  version=req.version(),
                  headers=req.headers().iter()
                                       .map(|(key, value)| format!("{}: {}\n", key.as_str(), str::from_utf8(value.as_bytes()).unwrap_or("Invalid UTF-8 value")))
                                       .collect::<String>()
                  );
        }
        Ok(Started::Done)
    }

    #[allow(unused)]
    fn finish(&self, _req: &mut HttpRequest<S>, resp: &HttpResponse) -> Finished {
        #[cfg(debug_assertions)]
        {
            use ::std::str;
            info!("HTTP: <-- {version:?} {status}{error}\n{headers}\n",
                  version=resp.version().unwrap_or(actix_web::http::Version::HTTP_11),
                  status=resp.status(),
                  error=resp.error().map(|error| format!(" Origin Error: {}", error)).unwrap_or("".to_string()),
                  headers=resp.headers().iter()
                                        .map(|(key, value)| format!("{}: {}\n", key.as_str(), str::from_utf8(value.as_bytes()).unwrap_or("Invalid UTF-8 value")))
                                        .collect::<String>()
                  );
        }

        Finished::Done
    }
}
