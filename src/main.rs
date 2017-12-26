#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate diesel;

extern crate irc;
extern crate tokio_core;
extern crate futures;

#[macro_use(slog_o, slog_info, slog_error, slog_warn, slog_log, slog_record, slog_debug, slog_trace, slog_record_static, slog_b, slog_kv)]
extern crate slog;
#[macro_use]
extern crate slog_scope;

use irc::client::server::{Server, IrcServer};
use irc::client::server::utils::ServerExt;
use futures::{Stream};

use std::io;
use std::rc;
use std::cell;

mod utils;
mod log;
mod config;
mod vndb;
mod handlers;
mod db;

use db::{
    Db,
    RunQueryDsl,
    QueryDsl
};

use utils::ResultExt;

macro_rules! try_in_loop {
    ($result:expr, $error_fmt:expr) => { match $result {
        Ok(result) => result,
        Err(error) => {
            error!($error_fmt, error);
            continue;
        }
    }};
    ($result:expr) => {{ try_in_loop!($result, "{}") }}
}

fn run() -> Result<i32, String> {
    let _log_guard = log::init();

    let db = Db::new()?;
    {
        let vns_count = Db::vns().count().get_result::<i64>(&*db).format_err("Cannot count entries in vns")?;
        let hooks_count = Db::hooks().count().get_result::<i64>(&*db).format_err("Cannot count entries in hooks")?;
        info!("DB stats: VNs {} | Hooks {}", vns_count, hooks_count);
    }

    let mut reactor = tokio_core::reactor::Core::new().format_err("Failed to init tokio loop")?;
    let tokio_handle = reactor.handle();
    let config = config::load()?;
    let vndb_client = vndb::Client::new(tokio_handle.clone()).format_err("Failed to init VNDB client")?;

    loop {
        let server = try_in_loop!(IrcServer::new_future(reactor.handle(), &config), "Error creating IRC Server connection: {}");
        let server = try_in_loop!(reactor.run(server), "Error creating IRC Server connection: {}");

        try_in_loop!(server.send_cap_req(&[irc::proto::caps::Capability::MultiPrefix]));
        try_in_loop!(server.identify());

        info!("Connected to {}:{} as {}",
              config.server.as_ref().unwrap(),
              config.port.as_ref().unwrap(),
              server.current_nickname());

        let message_handler = handlers::MessageHandler::new(server.clone(), vndb_client.clone(), db.clone());
        let server = server.stream().for_each(|message| {
            message_handler.dispatch(message)
        });

        if let Err(error) = reactor.run(server) {
            warn!("Bot disconnected. Error: {}", error);
        }
    }
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

