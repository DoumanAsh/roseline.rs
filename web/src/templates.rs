extern crate askama;
extern crate actix_web;
extern crate http;

extern crate vndb;

extern crate db;

use ::fmt;

pub use self::askama::Template;

use self::vndb::protocol::message::response::results::Vn as TypedVn;

use self::actix_web::dev::Handler;
use self::actix_web::{
    HttpResponse,
    HttpRequest,
    Responder,
    error
};
use self::actix_web::http::{
    ContentEncoding
};
use self::http::header;

use self::db::models;

#[derive(Template)]
#[template(path="_base.html")]
pub struct Base {
}

#[derive(Template)]
#[template(path="index.html")]
pub struct Index {
    _parent: Base,
    action: &'static str,
    caption: &'static str,
}

impl Index {
    pub fn new(action: &'static str, caption: &'static str) -> Self {
        Self {
            _parent: Base {},
            action,
            caption,
        }
    }
}

impl<S> Handler<S> for Index {
    type Result = HttpResponse;

    fn handle(&mut self, _: HttpRequest<S>) -> Self::Result {
        HttpResponse::Ok().content_type("text/html; charset=utf-8")
                          .content_encoding(ContentEncoding::Auto)
                          .body(self.render().unwrap().into_bytes())
    }
}

#[derive(Template)]
#[template(path="vn.html")]
pub struct Vn<'a> {
    _parent: Base,
    title: &'a str,
    hooks: Vec<models::Hook>
}

impl<'a> Vn<'a> {
    pub fn new(title: &'a str, hooks: Vec<models::Hook>) -> Self {
        Self {
            _parent: Base {},
            title,
            hooks
        }
    }
}

#[derive(Template)]
#[template(path="404.html")]
pub struct NotFound {
    _parent: Base,
}

impl NotFound {
    #[inline]
    pub fn new() -> Self {
        Self {
            _parent: Base {},
        }
    }

    #[inline]
    pub fn response(&self) -> HttpResponse {
        HttpResponse::NotFound().content_type("text/html; charset=utf-8")
                                .content_encoding(ContentEncoding::Auto)
                                .header(header::CACHE_CONTROL, "public, max-age=86400")
                                .body(self.render().unwrap().into_bytes())
    }
}

impl<S> Handler<S> for NotFound {
    type Result = HttpResponse;

    fn handle(&mut self, _: HttpRequest<S>) -> Self::Result {
        self.response()
    }
}

impl Responder for NotFound {
    type Item = HttpResponse;
    type Error = error::Error;

    fn respond_to(self, _: HttpRequest) -> Result<HttpResponse, error::Error> {
        Ok(self.response())
    }
}

#[derive(Template)]
#[template(path="500.html")]
pub struct InternalError<S: fmt::Display> {
    _parent: Base,
    description: S
}

impl<S: fmt::Display> InternalError<S> {
    #[inline]
    pub fn new(description: S) -> Self {
        Self {
            _parent: Base {},
            description
        }
    }

    #[inline]
    pub fn response(&self) -> HttpResponse {
        HttpResponse::InternalServerError().content_type("text/html; charset=utf-8")
                                           .content_encoding(ContentEncoding::Auto)
                                           .header(header::CACHE_CONTROL, "public, max-age=86400")
                                           .body(self.render().unwrap().into_bytes())
    }
}

impl<S: fmt::Display> Responder for InternalError<S> {
    type Item = HttpResponse;
    type Error = error::Error;

    fn respond_to(self, _: HttpRequest) -> Result<HttpResponse, error::Error> {
        Ok(self.response())
    }
}

#[derive(Template)]
#[template(path="search.html")]
pub struct Search<'a> {
    _parent: Base,
    title: &'a str,
    vns: Vec<models::Vn>
}

impl<'a> Search<'a> {
    pub fn new(title: &'a str, vns: Vec<models::Vn>) -> Self {
        Self {
            _parent: Base {},
            title,
            vns
        }
    }
}

#[derive(Template)]
#[template(path="vndb_results.html")]
pub struct VndbSearch<'a> {
    _parent: Base,
    title: &'a str,
    vns: &'a Vec<TypedVn>
}

impl<'a> VndbSearch<'a> {
    pub fn new(title: &'a str, vns: &'a Vec<TypedVn>) -> Self {
        Self {
            _parent: Base {},
            title,
            vns
        }
    }
}

pub trait ServeTemplate: Template {
    #[inline]
    fn serve(&self, status: http::StatusCode) -> HttpResponse {
        HttpResponse::build(status).content_type("text/html; charset=utf-8")
                                   .content_encoding(ContentEncoding::Auto)
                                   .body(self.render().unwrap().into_bytes())
    }

    #[inline]
    fn serve_ok(&self) -> HttpResponse {
        self.serve(http::StatusCode::OK)
    }
}

impl<S: Template> ServeTemplate for S {
}
