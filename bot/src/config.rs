extern crate irc;

use self::irc::client::data::config::Config as IrcConfig;

use ::std::env;
use ::std::path::PathBuf;
use ::std::ops::Deref;

use ::utils::ResultExt;

const NAME: &'static str = "roseline.toml";

///Retrieves path to configuration.
fn get_config() -> PathBuf {
    let mut result = env::current_exe().unwrap();

    result.set_file_name(NAME);

    result
}

pub struct Config {
    inner: IrcConfig,
    path: PathBuf
}

impl Deref for Config {
    type Target = IrcConfig;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl Config {
    #[inline]
    pub fn new() -> Result<Config, String> {
        let path = get_config();

        Ok(Config {
            inner: IrcConfig::load(&path).format_err("Failed to load bot's config")?,
            path
        })
    }

    //#[inline]
    //pub fn save(&self) -> Result<(), String> {
    //    self.inner.save(&self.path).format_err()
    //}
}

#[inline]
pub fn load() -> Result<Config, String> {
    Config::new()
}
