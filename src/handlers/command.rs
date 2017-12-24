extern crate futures;
extern crate regex;

use ::vndb;
use ::cell::Cell;

use self::futures::future;
use self::future::Future;

static HELP: &'static str = "Available commands: .ping, .vn, .hook";

pub enum CommandResult {
    Single(String),
    Multi(Vec<String>)
}

///Represents ongoing futures in [PendingCommand](struct.PendingCommand.html)
pub struct Ongoing {
    ///Futures in process
    pub jobs: future::JoinAll<Vec<vndb::VndbRequest>>,
    ///List of request types as short alias.
    pub types: Option<Vec<char>>
}

impl Ongoing {
    pub fn new(jobs: future::JoinAll<Vec<vndb::VndbRequest>>) -> Self {
        Self {
            jobs,
            types: None
        }
    }

    pub fn set_types(mut self, types: Vec<char>) -> Self {
        self.types = Some(types);
        self
    }
}

pub struct PendingCommand {
    cmd: Cell<Command>,
    vndb: vndb::Client,
    ongoing: Cell<Option<Ongoing>>
}

impl PendingCommand {
    #[inline]
    fn return_str(result: &str) -> futures::Poll<CommandResult, String> {
        Ok(futures::Async::Ready(CommandResult::Single(result.to_string())))
    }

    #[inline]
    fn return_string(result: String) -> futures::Poll<CommandResult, String> {
        Ok(futures::Async::Ready(CommandResult::Single(result)))
    }
}

macro_rules! try_option_or_continue {
    ($result:expr, $warn:expr) => { match $result {
        Some(result) => result,
        None => {
            warn!($warn);
            continue;
        }
    }}
}

macro_rules! handle_not_ready {
    ($ongoing:ident, $self:ident, $cmd:expr) => {{
        $self.ongoing.set(Some($ongoing));
        $self.cmd.set($cmd);
        Ok(futures::Async::NotReady)
    }}
}

macro_rules! handle_err {
    ($error:ident) => {{
        error!("Unexpected VNDB error while handling IRC command. Error={}", &$error);
        Err(format!("Unexpected VNDB error: {}", $error))
    }}
}

impl Future for PendingCommand {
    type Item = CommandResult;
    type Error = String;

    fn poll(&mut self) -> futures::Poll<Self::Item, Self::Error> {
        let cmd = self.cmd.take();

        match cmd {
            Command::Pong => Self::return_str("pong"),
            Command::Help => Self::return_str(HELP),
            Command::Vn(None) => Self::return_str("Which VN...?"),
            Command::VnSearch(title) => {
                let mut ongoing = self.ongoing.take().unwrap_or_else(|| {
                    let jobs = vec![
                        self.vndb.look_up_vn_by_title(&title)
                    ];
                    Ongoing::new(future::join_all(jobs))
                });

                match ongoing.jobs.poll() {
                    Ok(futures::Async::Ready(mut response)) => {
                        let response = response.drain(..).next().unwrap();
                        match response {
                            vndb::message::Response::Results(results) => {
                                let vn = match results.vn() {
                                    Ok(vn) => vn,
                                    Err(error) => {
                                        warn!("Expected to receive VN results, but couldn't parse into it. Error {}", error);
                                        return Self::return_str("VNDB has returned some strange results...");
                                    }
                                };

                                match vn.items.len() {
                                    0 => Self::return_str("No such VN could be found"),
                                    1 => {
                                        let vn = unsafe { vn.items.get_unchecked(0) };
                                        Self::return_string(format!("{} - https://vndb.org/v{}", vn.title.as_ref().unwrap(), vn.id))
                                    },
                                    num => Self::return_string(format!("There are too many hits='{}'. Try yourself -> https://vndb.org/v/all?sq={}",
                                                                       num, title))
                                }
                            },
                            other => {
                                warn!("Unexpected VNDB response on get: {:?}", other);
                                Self::return_str("Something wrong with VNDB...")
                            }
                        }
                    },
                    Ok(futures::Async::NotReady) => handle_not_ready!(ongoing, self, Command::VnSearch(title)),
                    Err(error) => handle_err!(error)
                }
            },
            Command::Vn(Some(title)) => {
                let mut ongoing = self.ongoing.take().unwrap_or_else(|| {
                    let jobs = vec![
                        self.vndb.get_vn_by_title(&title)
                    ];
                    Ongoing::new(future::join_all(jobs))
                });

                match ongoing.jobs.poll() {
                    Ok(futures::Async::Ready(mut response)) => {
                        let response = response.drain(..).next().unwrap();
                        match response {
                            vndb::message::Response::Results(results) => {
                                let vn = match results.vn() {
                                    Ok(vn) => vn,
                                    Err(error) => {
                                        warn!("Expected to receive VN results, but couldn't parse into it. Error {}", error);
                                        return Self::return_str("VNDB has returned some strange results...");
                                    }
                                };

                                match vn.items.len() {
                                    1 => {
                                        let vn = unsafe { vn.items.get_unchecked(0) };
                                        Self::return_string(format!("{} - https://vndb.org/v{}", vn.title.as_ref().unwrap(), vn.id))
                                    },
                                    _ => {
                                        self.cmd.set(Command::VnSearch(title));
                                        self.poll()
                                    }
                                }
                            },
                            other => {
                                warn!("Unexpected VNDB response on get: {:?}", other);
                                Self::return_str("Something wrong with VNDB...")
                            }
                        }
                    },
                    Ok(futures::Async::NotReady) => handle_not_ready!(ongoing, self, Command::Vn(Some(title))),
                    Err(error) => handle_err!(error)
                }
            },
            Command::VnRef(mut vns) => {
                let mut ongoing = self.ongoing.take().unwrap_or_else(|| {
                    let mut types = vec![];
                    let ongoing = vns.drain(..).map(|req| {
                        let typ = req.0.short().chars().next().unwrap();
                        types.push(typ);
                        self.vndb.get_by_id(req.0, req.1)
                    }).collect::<Vec<_>>();
                    Ongoing::new(future::join_all(ongoing)).set_types(types)
                });

                match ongoing.jobs.poll() {
                    Ok(futures::Async::Ready(response)) => {
                        let types = ongoing.types.unwrap();
                        let mut result = vec![];

                        for (idx, elem) in response.into_iter().enumerate() {
                            match elem {
                                vndb::message::Response::Results(results) => {
                                    let items = try_option_or_continue!(results.get("items"), "VNDB results is missing items field!");
                                    let item = try_option_or_continue!(items.get(0), "VNDB results's items is empty field!");
                                    let name = try_option_or_continue!(item.get("title").or(item.get("name")).or(item.get("username")),
                                    "VNDB results's item is missing title/name field!");

                                    let id = item.get("id").unwrap();
                                    let typ = unsafe { types.get_unchecked(idx) };
                                    result.push(format!("{}{}: {} - https://vndb.org/{}{}", typ, id, name, typ, id));
                                },
                                other => {
                                    warn!("Unexpected VNDB response on get: {:?}", other);
                                    continue;
                                }
                            }
                        }

                        Ok(futures::Async::Ready(CommandResult::Multi(result)))
                    },
                    Ok(futures::Async::NotReady) => handle_not_ready!(ongoing, self, Command::VnRef(vns)),
                    Err(error) => handle_err!(error)
                }
            },
            Command::None => unreachable!()
        }
    }
}

pub enum Command {
    None,
    Pong,
    Help,
    Vn(Option<String>),
    VnSearch(String),
    VnRef(Vec<(vndb::RequestType, u64)>)
}

impl Default for Command {
    fn default() -> Self {
        Command::None
    }
}

#[cfg(test)]
impl ::std::fmt::Debug for Command {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        match self {
            &Command::Pong => write!(f, stringify!(Command::Pong)),
            &Command::Help => write!(f, stringify!(Command::Help)),
            &Command::VnRef(_) => write!(f, "Command::VnRef(_)")
        }
    }
}

impl Command {
    pub fn from_str(text: &str) -> Option<Command> {
        lazy_static! {
            static ref EXTRACT_CMD: regex::Regex = regex::Regex::new(" *\\.([^ ]*) *(.*)").unwrap();
            static ref EXTRACT_REFERENCE: regex::Regex = regex::Regex::new("([vcrpu])([0-9]+)").unwrap();
        }

        if let Some(captures) = EXTRACT_CMD.captures(text) {
            let cmd = captures.get(1);
            let cmd = cmd.map(|cmd| cmd.as_str());

            match cmd {
                Some("ping") => Some(Command::Pong),
                Some("help") => Some(Command::Help),
                Some("vn") => Some(Command::Vn(captures.get(2).map(|name| name.as_str().to_owned()))),
                _ => None
            }
        }
        else if EXTRACT_REFERENCE.is_match(text) {
            let mut result = vec![];

            for capture in EXTRACT_REFERENCE.captures_iter(text) {
                let typ = match capture.get(1) {
                    Some(typ) => match typ.as_str() {
                        "v" => vndb::RequestType::vn(),
                        "c" => vndb::RequestType::character(),
                        "r" => vndb::RequestType::release(),
                        "p" => vndb::RequestType::release(),
                        "u" => vndb::RequestType::user(),
                        _ => continue
                    },
                    None => continue
                };
                let id = match capture.get(2) {
                    Some(id) => match id.as_str().parse::<u64>() {
                        Ok(result) if result > 0 => result,
                        _ => continue
                    },
                    None => continue
                };

                result.push((typ, id))
            }
            Some(Command::VnRef(result))
        }
        else {
            None
        }
    }

    ///Executes command and returns future that resolves into response to user.
    pub fn exec(self, vndb: vndb::Client) -> PendingCommand {
        PendingCommand {
            cmd: Cell::new(self),
            vndb,
            ongoing: Cell::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::vndb;
    use super::Command;

    macro_rules! assert_result {
        ($result:expr, $($expected:tt)+) => { assert_result!(ret => (), $result, $($expected)+) };
        (ret => $ret:tt, $result:expr, $($expected:tt)+) => { match $result {
            $($expected)+ => $ret,
            other => panic!(format!("Unexpected result={:?}, should be {:?}", other, stringify!($($expected)+)))
        }}
    }

    #[test]
    fn should_cmd_pong() {
        let result = Command::from_str(" .ping");
        assert_result!(result, Some(Command::Pong));

        let result = Command::from_str("ping");
        assert_result!(result, None);
    }

    #[test]
    fn should_cmd_help() {
        let result = Command::from_str(".help");
        assert_result!(result, Some(Command::Help));

        let result = Command::from_str(".relp");
        assert_result!(result, None);
    }

    #[test]
    fn should_cmd_vn_ref() {
        let expected_refs = vec![
            (vndb::RequestType::vn(), 1u64),
            (vndb::RequestType::vn(), 2u64),
            (vndb::RequestType::user(), 55u64),
            (vndb::RequestType::character(), 25u64)
        ];
        let result = Command::from_str("v1 d2 v2 u55 c25");
        let result = assert_result!(ret => res, result, Some(Command::VnRef(ref res)) if res.len() == expected_refs.len());

        for (idx, refs) in result.iter().enumerate() {
            assert_eq!(refs.0.short(), expected_refs[idx].0.short());
            assert_eq!(refs.1, expected_refs[idx].1);
        }

        let result = Command::from_str("g1 d2 g2");
        let _result = assert_result!(result, None);
    }
}
