extern crate actix_web;
extern crate memmap;
extern crate etag;

use std::io;
use std::fs;
use std::path;

use self::actix_web::{
    HttpRequest,
    HttpResponse,
    HttpMessage,
    http,
    Body
};
use self::actix_web::http::{
    Method,
};
use self::http::header;

use ::templates;

///Matches given ETag against If-None-Match header.
///
///Returns true if matching value found in header.
fn if_none_match(etag: &etag::EntityTag, headers: &header::HeaderMap) -> bool {
    match headers.get(header::IF_NONE_MATCH).and_then(|header| header.to_str().ok()) {
        Some(header) => {
            for header_tag in header.split(',').map(|tag| tag.trim()) {
                match header_tag.parse::<etag::EntityTag>() {
                    Ok(header_tag) => match etag.weak_eq(&header_tag) {
                        true => return true,
                        false => (),
                    },
                    Err(_) => ()
                }
            }
            false
        },
        None => false
    }
}

///Serves dynamic file using mmap with content disposition as attachment.
fn serve_file_save_as<P: AsRef<path::Path>, S>(path: P, req: &HttpRequest<S>) -> io::Result<HttpResponse> {
    let path = path.as_ref();
    let file = fs::File::open(&path)?;

    let metadata = file.metadata()?;
    let file_etag = etag::EntityTag::from_file_meta(&metadata);
    if if_none_match(&file_etag, req.headers()) {
        return Ok(HttpResponse::NotModified().body(actix_web::Body::Empty));
    }


    let content_dispotion = match path.file_name().and_then(|name| name.to_str()) {
        Some(file_name) => format!("attachment; filename=\"{}\"", file_name),
        None => "attachment".to_owned()
    };

    let body = {
        let mmap = unsafe { memmap::Mmap::map(&file)? };
        actix_web::Binary::from_slice(&mmap[..])
    };

    Ok(HttpResponse::Ok().content_type("application/octet-stream")
                         .content_encoding(header::ContentEncoding::Auto)
                         .header(header::CONTENT_DISPOSITION, content_dispotion)
                         .header(header::ETAG, file_etag.to_string())
                         .body(body).into())
}

///Serves static files with max-age 1 week
fn serve<B: Into<Body>>(bytes: B, content_type: &str, encoding: header::ContentEncoding) -> HttpResponse {
    HttpResponse::Ok().content_type(content_type)
                      .content_encoding(encoding)
                      .header(header::CACHE_CONTROL, "public, max-age=604800")
                      .body(bytes.into())
}

fn app_bundle_css<S>(_: &HttpRequest<S>) -> HttpResponse {
    const CSS: &'static [u8] = include_bytes!("../../static/main.css");
    serve(CSS, "text/css; charset=utf-8", header::ContentEncoding::Auto)
}

//fn app_bundle_js<S>(_: &HttpRequest<S>) -> HttpResponse {
//    const JS: &'static [u8] = include_bytes!("../../static/app.bundle.js");
//    serve(JS, "application/javascript; charset=utf-8", header::ContentEncoding::Auto)
//}

fn roseline_png<S>(_: &HttpRequest<S>) -> HttpResponse {
    const IMG: &'static [u8] = include_bytes!("../../static/Roseline.png");
    serve(IMG, "image/png", header::ContentEncoding::Identity)
}

fn favicon<S>(_: &HttpRequest<S>) -> HttpResponse {
    const IMG: &'static [u8] = include_bytes!("../../static/favicon.png");
    serve(IMG, "image/png", header::ContentEncoding::Identity)
}

fn ith_vnr<S>(_: &HttpRequest<S>) -> HttpResponse {
    const ZIP: &'static [u8] = include_bytes!("../../static/ITHVNR.zip");
    serve(ZIP, "application/zip", header::ContentEncoding::Identity)
}

fn db_dump<S>(req: &HttpRequest<S>) -> actix_web::Either<HttpResponse, templates::InternalError<io::Error>> {
    extern crate db;

    match serve_file_save_as(db::PATH, &req) {
        Ok(res) => actix_web::Either::A(res),
        Err(error) => {
            error!("Unable to open DB: {}. Error: {}", db::PATH, error);
            actix_web::Either::B(templates::InternalError::new(error))
        }
    }
}


pub fn config<S: 'static>(app: actix_web::App<S>) -> actix_web::App<S> {
    app.resource("/app.bundle.css", |res| {
        res.method(Method::GET).f(app_bundle_css);
        res.route().f(super::not_allowed);
    }).resource("/Roseline.png", |res| {
        res.method(Method::GET).f(roseline_png);
        res.route().f(super::not_allowed);
    }).resource("/favicon.png", |res| {
        res.method(Method::GET).f(favicon);
        res.route().f(super::not_allowed);
    }).scope("/download", |scope| {
        scope.resource("/ITHVNR.zip", |res| {
            res.method(Method::GET).f(ith_vnr);
            res.route().f(super::not_allowed);
        }).resource("/roseline.db", |res| {
            res.method(Method::GET).f(db_dump);
            res.route().f(super::not_allowed);
        }).default_resource(|res| {
            res.route().h(templates::NotFound::new());
        })
    })
}

#[cfg(test)]
mod tests {
    extern crate http;
    extern crate etag;
    use super::if_none_match;

    fn get_header(value: &'static str) -> http::HeaderMap {
        let mut headers = http::HeaderMap::new();
        headers.insert(http::header::IF_NONE_MATCH, http::header::HeaderValue::from_static(value));
        headers
    }

    #[test]
    fn test_if_none_match_hit() {
        let etag = etag::EntityTag::new(true, "12345".to_owned());

        let headers = get_header("\"1234\", W/\"12345\"");

        assert!(if_none_match(&etag, &headers));
    }

    #[test]
    fn test_if_none_match_miss() {
        let etag = etag::EntityTag::new(true, "12345".to_owned());

        let headers = get_header("\"1234\", W/\"1234\"");

        assert!(!if_none_match(&etag, &headers));
    }

}
