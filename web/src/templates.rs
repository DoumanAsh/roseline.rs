extern crate askama;

extern crate db;

pub use self::askama::Template;

use self::db::models;

#[derive(Template)]
#[template(path="_base.html")]
pub struct Base {
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
