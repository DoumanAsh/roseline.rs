#[macro_use]
extern crate askama;
#[macro_use(slog_info, slog_error, slog_log, slog_record, slog_record_static, slog_b, slog_kv)]
extern crate slog;
#[macro_use]
extern crate slog_scope;
#[macro_use]
extern crate serde_derive;

extern crate utils;

use utils::log;

use std::io;
use std::fmt;
use std::cmp;
use std::net;

mod templates;
mod server;

fn main() {
    utils::ssl::init();
    let _log_guard = log::init();

    server::start();
}

