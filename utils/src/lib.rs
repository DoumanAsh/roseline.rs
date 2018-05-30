#[macro_use(slog_o, slog_kv)]
extern crate slog;
#[macro_use(set_panic_message)]
extern crate lazy_panic;

pub mod log;
pub mod ssl;

use std::fmt;
use std::time;

pub mod duration;

///Extension to std Result.
pub trait ResultExt<T, E> {
    ///Formats error to string.
    fn format_err(self, prefix: &str) -> Result<T, String>;
}

impl<T, E: fmt::Display> ResultExt<T, E> for Result<T, E> {
    fn format_err(self, prefix: &str) -> Result<T, String> {
        self.map_err(|error| format!("{}. Error: {}", prefix, error))
    }
}
