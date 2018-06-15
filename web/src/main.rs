#[macro_use]
extern crate askama;
#[macro_use]
extern crate log;
#[macro_use]
extern crate serde_derive;

extern crate utils;

use std::io;
use std::fmt;
use std::cmp;
use std::net;

mod templates;
mod server;

fn main() {
    utils::ssl::init();
    let _log_guard = utils::log::init();

    server::start();
}

