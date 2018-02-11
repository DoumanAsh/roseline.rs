#[macro_use(slog_info, slog_error, slog_warn, slog_log, slog_record, slog_debug, slog_trace, slog_record_static, slog_b, slog_kv)]
extern crate slog;
#[macro_use]
extern crate slog_scope;
#[macro_use]
extern crate lazy_static;

extern crate actix;

extern crate utils;
extern crate actors;

use utils::log;

use actix::{Supervisor};

use std::fmt;
use std::mem;

mod config;
mod command;
mod irc;

fn run() -> Result<i32, String> {
    let _log_guard = log::init();

    let config = config::load()?;
    let system = actix::System::new("roseline");

    let db: actix::SyncAddress<_> = actors::db::Db::start_threaded(1);
    let vndb: actix::Address<_> = Supervisor::start(|_| actors::vndb::Vndb::new());
    let irc: actix::Address<_> = Supervisor::start(|_| irc::Irc::new(config, vndb, db));

    Ok(system.run())
}

fn main() {
    use std::process::exit;

    let code: i32 = match run() {
        Ok(res) => res,
        Err(error) => {
            eprintln!("{}", error);
            1
        }
    };

    exit(code);
}

