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

use actix::{Supervisor, Actor};

use std::collections;
use std::thread;
use std::fmt;
use std::mem;

mod config;
mod command;
mod irc;
mod discord;

fn run() -> Result<i32, String> {
    let _log_guard = log::init();

    let config = config::load()?;
    let system = actix::System::new("roseline");

    let executor: actix::Addr<actix::Syn, _> = actors::exec::Executor::default_threads(2).start();
    let executor2 = executor.clone();
    let _irc: actix::Addr<actix::Unsync, _> = Supervisor::start(move |_| irc::Irc::new(config, executor2));

    thread::spawn(move || {
        loop {
            let mut client = discord::client(executor.clone());
            if let Err(why) = client.start() {
                println!("An error occurred while running the client: {:?}", why);
            }
        }
    });

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

