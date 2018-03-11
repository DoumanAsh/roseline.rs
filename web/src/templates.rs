extern crate askama;
extern crate actix_web;
extern crate http;

extern crate vndb;

extern crate db;

pub use self::askama::Template;

use self::vndb::protocol::message::response::results::Vn as TypedVn;

use self::http::Error as HttpError;
use self::actix_web::dev::Handler;
use self::actix_web::{
    HttpResponse,
    HttpRequest
};
use self::actix_web::headers::{
    ContentEncoding
};

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
    type Result = Result<HttpResponse, HttpError>;

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
    pub fn new() -> Self {
        Self {
            _parent: Base {},
        }
    }
}

impl<S> Handler<S> for NotFound {
    type Result = Result<HttpResponse, HttpError>;

    fn handle(&mut self, _: HttpRequest<S>) -> Self::Result {
        HttpResponse::NotFound().content_type("text/html; charset=utf-8")
                                .content_encoding(ContentEncoding::Auto)
                                .body(self.render().unwrap().into_bytes())
    }
}

#[derive(Template)]
#[template(path="500.html")]
pub struct InternalError {
    _parent: Base,
    description: String
}

impl InternalError {
    pub fn new(description: String) -> Self {
        Self {
            _parent: Base {},
            description
        }
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
