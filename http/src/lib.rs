extern crate yukikaze;
extern crate actix;
extern crate regex;
#[macro_use]
extern crate log;

use self::yukikaze::client::config::Config;
pub use self::yukikaze::client::request::multipart;
pub use self::yukikaze::client::request::Builder;
pub use self::yukikaze::client::request::tags::IfModifiedSince;
pub use self::yukikaze::client::Request;
pub use self::yukikaze::{encoding, futures};
pub use self::yukikaze::httpdate::HttpDate;
pub use self::yukikaze::futures::{Future, IntoFuture};
pub use self::yukikaze::header;
pub use self::yukikaze::http::Method;
pub use self::yukikaze::mime::Mime;
pub use self::yukikaze::rt::{AutoClient, AutoRuntime, Guard};
pub use yukikaze::client::response::errors::ResponseError;

struct Conf;

impl Config for Conf {
}

pub fn init() {
    yukikaze::rt::set_with_config::<Conf>();
}

pub mod kouryaku;
