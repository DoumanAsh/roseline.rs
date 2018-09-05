#[macro_use]
extern crate log;
#[macro_use]
extern crate lazy_static;

extern crate actix;

extern crate utils;
extern crate actors;
extern crate http;

use actix::{Supervisor, Actor};

use std::collections;
use std::thread;
use std::fmt;

mod config;
mod command;
mod irc;
mod discord;

fn run() -> Result<i32, String> {
    utils::ssl::init();
    http::init();

    let _log_guard = utils::log::init();

    let config = config::load()?;
    let system = actix::System::new("roseline");

    let executor: actix::Addr<_> = actors::exec::Executor::default_threads(2).start();
    let executor2 = executor.clone();
    let _irc: actix::Addr<_> = Supervisor::start(move |_| irc::Irc::new(config, executor2));

    let kouryaku = actix::System::current().registry().get::<http::kouryaku::Kouryaku>();

    thread::spawn(move || {
        loop {
            let mut client = discord::client(executor.clone(), kouryaku.clone());
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

