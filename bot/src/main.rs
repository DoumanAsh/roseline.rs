#[macro_use]
extern crate lazy_static;

extern crate irc;
extern crate futures;

#[macro_use(slog_info, slog_error, slog_warn, slog_log, slog_record, slog_debug, slog_trace, slog_record_static, slog_b, slog_kv)]
extern crate slog;
#[macro_use]
extern crate slog_scope;

extern crate utils;
extern crate db;
extern crate int_vndb;

use int_vndb as vndb;
use utils::log;

use irc::client::reactor::IrcReactor;
use irc::client::ext::ClientExt;

use std::cell;

mod config;
mod handlers;

use db::{
    Db,
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
        let vns = db.count_vns().format_err("Cannot count entries in vns table")?;
        let hooks = db.count_hooks().format_err("Cannot count entries in vns table")?;
        info!("DB stats: VNs {} | Hooks {}", vns, hooks);
    }

    let mut reactor = IrcReactor::new().format_err("Failed to init event loop")?;
    let tokio_handle = reactor.inner_handle();
    let config = config::load()?;
    let vndb_client = vndb::Client::new(tokio_handle.clone()).format_err("Failed to init VNDB client")?;

    loop {
        let client = try_in_loop!(reactor.prepare_client_and_connect(&config), "Error connecting to IRC Server: {}");

        try_in_loop!(client.send_cap_req(&[irc::proto::caps::Capability::MultiPrefix]));
        try_in_loop!(client.identify());

        info!("Connecting to {}:{} as {}",
              config.server.as_ref().unwrap(),
              config.port.as_ref().unwrap(),
              client.current_nickname());

        let message_handler = handlers::MessageHandler::new(client.clone(), vndb_client.clone(), db.clone());
        reactor.register_client_with_handler(client, move |_, message| {
            message_handler.dispatch(message)
        });

        if let Err(error) = reactor.run() {
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

