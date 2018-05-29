extern crate slog_scope;
extern crate slog_term;
extern crate slog_async;
extern crate slog_stdlog;
extern crate lazy_panic;

use ::slog;
use ::slog::Drain;

///Initializes logging facilities
pub fn init() -> slog_scope::GlobalLoggerGuard {
    set_panic_message!(lazy_panic::formatter::Simple);

    let decorator = slog_term::TermDecorator::new().build();
    let drain = slog_term::FullFormat::new(decorator).build().fuse();
    let drain = slog_async::Async::new(drain).chan_size(256).build().fuse();
    let logger = slog::Logger::root(drain, slog_o!(
            "location" => slog::FnValue(move |info| { format!("{}:{}", info.file(), info.line()) })
    ));

    let _log_guard = slog_stdlog::init().unwrap();
    return slog_scope::set_global_logger(logger);
}
