#[macro_use]
extern crate askama;
#[macro_use(slog_info, slog_warn, slog_error, slog_log, slog_record, slog_record_static, slog_b, slog_kv)]
extern crate slog;
#[macro_use]
extern crate slog_scope;

extern crate utils;

use utils::log;

use std::cmp;

mod templates;
mod server;

fn run() -> Result<i32, String> {
    let _log_guard = log::init();

    server::start();

    Ok(0)
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

