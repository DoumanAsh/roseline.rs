extern crate actix_web;
extern crate http;
extern crate bytes;

use self::http::header;
use self::http::uri;
use self::actix_web::{HttpRequest, Result, HttpResponse};
use self::actix_web::middleware::{Middleware, Started, Response};
pub use self::actix_web::middleware::{Logger};

use std::marker::PhantomData;

///Default headers middleware
pub struct DefaultHeaders;

impl<S> Middleware<S> for DefaultHeaders {
    fn response(&self, _: &mut HttpRequest<S>, mut resp: HttpResponse) -> Result<Response> {
        const DEFAULT_HEADERS: [(header::HeaderName, &'static str); 4] = [
            (header::SERVER, "Roseline"),
            (header::X_DNS_PREFETCH_CONTROL, "off"),
            (header::X_XSS_PROTECTION, "1; mode=block"),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        ];

        {
            let headers = resp.headers_mut();
            for (key, value) in DEFAULT_HEADERS.iter() {
                if !headers.contains_key(key) {
                    headers.insert(key, header::HeaderValue::from_static(value));
                }
            }
        }

        Ok(Response::Done(resp))
    }
}

pub trait SlashHandler {
    ///Normalizes path, if necessary.
    fn normalize(path: &str) -> Option<bytes::Bytes>;
}

///Remove trailing slash during normalization
pub struct RemoveTrailingSlach;
impl SlashHandler for RemoveTrailingSlach {
    fn normalize(path: &str) -> Option<bytes::Bytes> {
        if !path.ends_with("/") {
            return None;
        }

        let path = &path[..path.len()-1];
        Some(bytes::Bytes::from(path))
    }
}

#[allow(dead_code)]
///Adds trailing slash during normalization
pub struct AddTrailingSlach;
impl SlashHandler for AddTrailingSlach {
    fn normalize(path: &str) -> Option<bytes::Bytes> {
        if path.ends_with("/") {
            return None;
        }

        let mut path = bytes::Bytes::from(path);
        path.extend_from_slice(b"/");
        Some(path)
    }
}

///Normalizes URL's trailing slashes.
pub struct NormalizeUrl<N: SlashHandler> {
    _norm: PhantomData<N>
}

impl<N: SlashHandler> NormalizeUrl<N> {
    pub fn new() -> Self {
        Self {
            _norm: PhantomData
        }
    }

    ///Returns new URL, if normalization is needed.
    pub fn normalize(url: &uri::Uri) -> Option<uri::Uri> {
        let path = {
            let path_and_query = if let Some(result) = url.path_and_query() {
                result
            } else {
                return None
            };

            let mut path = match N::normalize(path_and_query.path()) {
                Some(result) => result,
                None => return None
            };

            if let Some(query) = path_and_query.query() {
                path.extend_from_slice(query.as_bytes());
            };

            match uri::PathAndQuery::from_shared(path) {
                Ok(result) => result,
                Err(error) => {
                    warn!("Unable to create normalized URL's path. Error: {}", error);
                    return None;
                }
            }
        };

        let mut new_parts = uri::Parts::default();
        new_parts.scheme = url.scheme_part().cloned();
        new_parts.authority = url.authority_part().cloned();
        new_parts.path_and_query = Some(path);

        match uri::Uri::from_parts(new_parts) {
            Ok(result) => Some(result),
            Err(error) => {
                warn!("Unable to create normalized URL. Error: {}", error);
                None
            }
        }
    }
}

///Available URI normalizers.
pub mod normalizer {
    ///Removing trailing slash middleware
    pub type RemoveTrailingSlach = super::NormalizeUrl<super::RemoveTrailingSlach>;
    #[allow(dead_code)]
    ///Adding trailing slash middleware
    pub type AddTrailingSlach = super::NormalizeUrl<super::AddTrailingSlach>;
}

impl<S, N: SlashHandler + 'static> Middleware<S> for NormalizeUrl<N> {
    fn start(&self, req: &mut HttpRequest<S>) -> Result<Started> {
        if let Some(new_url) = Self::normalize(req.uri()) {
            *(req.uri_mut()) = new_url;
        }

        Ok(Started::Done)
    }
}

#[cfg(test)]
mod tests {
    use super::normalizer;
    use super::uri;

    #[test]
    fn remove_trailing_slash() {
        type Norm = normalizer::RemoveTrailingSlach;

        let url = "http://localhost/vndb/".parse::<uri::Uri>().expect("Valid URI");
        let new_url = Norm::normalize(&url).expect("To remove trailing slash");
        assert_eq!(new_url.path(), "/vndb");

        let url = "http://localhost/vndb".parse::<uri::Uri>().expect("Valid URI");
        let new_url = Norm::normalize(&url);
        assert!(new_url.is_none());
    }

    #[test]
    fn add_trailing_slash() {
        type Norm = normalizer::AddTrailingSlach;

        let url = "http://localhost/vndb/".parse::<uri::Uri>().expect("Valid URI");
        let new_url = Norm::normalize(&url);
        assert!(new_url.is_none());

        let url = "http://localhost/vndb".parse::<uri::Uri>().expect("Valid URI");
        let new_url = Norm::normalize(&url).expect("To remove trailing slash");
        assert_eq!(new_url.path(), "/vndb/");
    }

}
