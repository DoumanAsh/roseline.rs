extern crate actix_web;

use self::actix_web::{
    HttpRequest,
    HttpResponse,
    http,
    Body
};

use self::http::header;

///Serves static files with max-age 1 week
pub fn serve<B: Into<Body>>(bytes: B, content_type: &str, encoding: header::ContentEncoding) -> HttpResponse {
    HttpResponse::Ok().content_type(content_type)
                      .content_encoding(encoding)
                      .header(header::CACHE_CONTROL, "public, max-age=604800")
                      .body(bytes.into())
}

pub fn app_bundle_css<S>(_: HttpRequest<S>) -> HttpResponse {
    const CSS: &'static [u8] = include_bytes!("../../static/app.bundle.css");
    serve(CSS, "text/css; charset=utf-8", header::ContentEncoding::Auto)
}

pub fn app_bundle_js<S>(_: HttpRequest<S>) -> HttpResponse {
    const JS: &'static [u8] = include_bytes!("../../static/app.bundle.js");
    serve(JS, "application/javascript; charset=utf-8", header::ContentEncoding::Auto)
}

pub fn roseline_png<S>(_: HttpRequest<S>) -> HttpResponse {
    const IMG: &'static [u8] = include_bytes!("../../static/Roseline.png");
    serve(IMG, "image/png", header::ContentEncoding::Identity)
}

pub fn favicon<S>(_: HttpRequest<S>) -> HttpResponse {
    const IMG: &'static [u8] = include_bytes!("../../static/favicon.png");
    serve(IMG, "image/png", header::ContentEncoding::Identity)
}
