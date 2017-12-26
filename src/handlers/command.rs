extern crate futures;
extern crate regex;

use ::db::Db;
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
    db: Db,
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

    #[inline]
    ///Takes ongoing job to search VN or schedule new one
    fn search_vn_if(&self, title: &str) -> Ongoing {
        self.ongoing.take().unwrap_or_else(|| {
            let jobs = vec![
                self.vndb.look_up_vn_by_title(&title)
            ];
            Ongoing::new(future::join_all(jobs))
        })
    }

    ///Takes ongoing job to retrieve VN by title or schedule new one
    fn get_vn_if(&self, title: &str) -> Ongoing {
        self.ongoing.take().unwrap_or_else(|| {
            let jobs = vec![
                self.vndb.get_vn_by_title(&title)
            ];
            Ongoing::new(future::join_all(jobs))
        })
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

macro_rules! try_parse_vn {
    ($result:expr) => {{
        match $result {
            Ok(vn) => vn,
            Err(error) => {
                warn!("Expected to receive VN results but couldn't parse into it. Error {}", error);
                return Self::return_str("VNDB has returned some strange results...");
            }
        }
    }}
}

macro_rules! try_ongoing_job {
    ($result:expr, $ongoing:ident, $self: ident, $cmd:expr) => { match $result {
        Ok(futures::Async::Ready(response)) => response,
        Ok(futures::Async::NotReady) => {
            $self.ongoing.set(Some($ongoing));
            $self.cmd.set($cmd);
            return Ok(futures::Async::NotReady);
        },
        Err(error) => {
            error!("Unexpected VNDB error while handling IRC command. Error={}", &error);
            return Err(format!("Unexpected VNDB error: {}", error));
        }
    }}
}

impl Future for PendingCommand {
    type Item = CommandResult;
    type Error = String;

    fn poll(&mut self) -> futures::Poll<Self::Item, Self::Error> {
        let cmd = self.cmd.take();

        match cmd {
            //Retrieve Hook from DB by id
            Command::GetHookById(id) => {
                let vn = match self.db.get_vn(id as i32) {
                    Ok(Some(id)) => id,
                    Ok(None) => return Self::return_str("No hook exists"),
                    Err(error) => {
                        error!("DB error: {}", error);
                        return Self::return_str("My database has problems...");
                    }
                };

                let hooks = match self.db.get_hooks(&vn) {
                    Ok(hooks) => {
                        if hooks.is_empty() {
                            return Self::return_str("No hooks registered");
                        }
                        hooks
                    },
                    Err(error) => {
                        error!("DB error: {}", error);
                        return Self::return_str("My database has problems...");
                    }
                };

                let result = match hooks.len() {
                    1 => {
                        let hook = unsafe { hooks.get_unchecked(0) };
                        format!("{} - {}", vn.title, hook.code)
                    }
                    _ => {
                        let mut result = format!("{} - ", vn.title);

                        for hook in hooks {
                            result.push_str(&format!("{}: {} | ", hook.version, hook.code));
                        }

                        result.pop();
                        result.pop();
                        result.pop();

                        result
                    }
                };

                Self::return_string(result)
            },
            //Looks up VN's ID by searching VNDB.
            Command::GetHookSerachVn(title) => {
                let mut ongoing = self.search_vn_if(&title);

                let mut response = try_ongoing_job!(ongoing.jobs.poll(), ongoing, self, Command::GetHookSerachVn(title));
                let mut response = response.drain(..);
                match response.next() {
                    Some(vndb::message::Response::Results(results)) => {
                        let vn = try_parse_vn!(results.vn());
                        match vn.items.len() {
                            0 => Self::return_str("No such VN could be found"),
                            1 => {
                                let vn = unsafe { vn.items.get_unchecked(0) };
                                self.cmd.set(Command::GetHookById(vn.id));
                                self.poll()
                            },
                            num => Self::return_string(format!("There are too many hits='{}'. Try yourself -> https://vndb.org/v/all?sq={}",
                                                               num, title))
                        }
                    }
                    other => {
                        warn!("Unexpected VNDB response on get: {:?}", other);
                        return Self::return_str("Something wrong with VNDB...")
                    }
                }
            }
            Command::GetHookHelp => Self::return_str("For which VN...?"),
            //Gets VN's ID by exact title.
            Command::GetHook(title) => {
                let mut ongoing = self.get_vn_if(&title);

                let mut response = try_ongoing_job!(ongoing.jobs.poll(), ongoing, self, Command::GetHook(title));
                let mut response = response.drain(..);
                match response.next() {
                    Some(vndb::message::Response::Results(results)) => {
                        let vn = try_parse_vn!(results.vn());

                        match vn.items.len() {
                            1 => {
                                let vn = unsafe { vn.items.get_unchecked(0) };
                                self.cmd.set(Command::GetHookById(vn.id));
                                self.poll()
                            },
                            _ => {
                                self.cmd.set(Command::GetHookSerachVn(title));
                                self.poll()
                            }
                        }
                    }
                    other => {
                        warn!("Unexpected VNDB response on get: {:?}", other);
                        return Self::return_str("Something wrong with VNDB...")
                    }
                }
            },
            Command::VnSearch(title) => {
                let mut ongoing = self.search_vn_if(&title);

                let mut response = try_ongoing_job!(ongoing.jobs.poll(), ongoing, self, Command::VnSearch(title));
                let response = response.drain(..).next().unwrap();
                match response {
                    vndb::message::Response::Results(results) => {
                        let vn = try_parse_vn!(results.vn());

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
            Command::Vn(None) => Self::return_str("Which VN...?"),
            Command::Vn(Some(title)) => {
                let mut ongoing = self.get_vn_if(&title);

                let mut response = try_ongoing_job!(ongoing.jobs.poll(), ongoing, self, Command::Vn(Some(title)));
                let response = response.drain(..).next().unwrap();
                match response {
                    vndb::message::Response::Results(results) => {
                        let vn = try_parse_vn!(results.vn());

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

                let response = try_ongoing_job!(ongoing.jobs.poll(), ongoing, self, Command::VnRef(vns));
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
            Command::Pong => Self::return_str("pong"),
            Command::Help => Self::return_str(HELP),
            Command::None => unreachable!()
        }
    }
}

pub enum Command {
    None,
    Pong,
    Help,
    GetHookHelp,
    Vn(Option<String>),
    GetHookById(u64),
    GetHookSerachVn(String),
    GetHook(String),
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
            &Command::None => write!(f, stringify!(Command::None)),
            &Command::Pong => write!(f, stringify!(Command::Pong)),
            &Command::Help => write!(f, stringify!(Command::Help)),
            &Command::GetHookHelp => write!(f, stringify!(Command::GetHookHelp)),
            &Command::Vn(ref payload) => write!(f, "Command::Vn({:?})", payload),
            &Command::GetHookById(ref payload) => write!(f, "Command::GetHookById({:?})", payload),
            &Command::GetHook(ref payload) => write!(f, "Command::GetHook({:?})", payload),
            &Command::GetHookSerachVn(ref payload) => write!(f, "Command::GetHookSerachVn({:?})", payload),
            &Command::VnSearch(ref payload) => write!(f, "Command::VnSearch({:?})", payload),
            &Command::VnRef(_) => write!(f, "Command::VnRef(_)"),
        }
    }
}

impl Command {
    pub fn from_str(text: &str) -> Option<Command> {
        lazy_static! {
            static ref EXTRACT_CMD: regex::Regex = regex::Regex::new("\\s*\\.([^\\s]*)(\\s+(.+))*").unwrap();
            //static ref EXTRACT_CMD: regex::Regex = regex::Regex::new(" *\\.([^\\s]*) *(.*)").unwrap();
            static ref EXTRACT_REFERENCE: regex::Regex = regex::Regex::new("(^|\\s)([vcrpu])([0-9]+)").unwrap();
            static ref EXTRACT_VN_ID: regex::Regex = regex::Regex::new("^v([0-9]+)$").unwrap();
        }

        const CMD_IDX: usize = 1;
        const ARG_IDX: usize = 3;

        if let Some(captures) = EXTRACT_CMD.captures(text) {
            let cmd = captures.get(CMD_IDX);
            let cmd = cmd.map(|cmd| cmd.as_str());

            match cmd {
                Some("ping") => Some(Command::Pong),
                Some("help") => Some(Command::Help),
                Some("vn") => Some(Command::Vn(captures.get(ARG_IDX).map(|name| name.as_str().to_owned()))),
                Some("hook") => {
                    let arg = match captures.get(ARG_IDX) {
                        Some(arg) => arg,
                        None => return Some(Command::GetHookHelp),
                    };

                    let arg = arg.as_str().trim();
                    match EXTRACT_VN_ID.captures(arg).map(|cap| cap.get(1).map(|cap| cap.as_str()).map(|cap| cap.parse::<u64>())) {
                        Some(Some(Ok(capture))) => Some(Command::GetHookById(capture)),
                        _ => Some(Command::GetHook(arg.to_owned())),
                    }
                }
                _ => None
            }
        }
        else if EXTRACT_REFERENCE.is_match(text) {
            let mut result = vec![];

            for capture in EXTRACT_REFERENCE.captures_iter(text) {
                let typ = match capture.get(2) {
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
                let id = match capture.get(3) {
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
    pub fn exec(self, vndb: vndb::Client, db: Db) -> PendingCommand {
        PendingCommand {
            cmd: Cell::new(self),
            vndb,
            db,
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

        let result = Command::from_str("2v2");
        let _result = assert_result!(result, None);
    }

    #[test]
    fn should_cmd_get_hook() {
        let result = Command::from_str(".hook");
        assert_result!(result, Some(Command::GetHookHelp));

        let result = Command::from_str(".hook Some Title");
        let result = assert_result!(ret => res, result, Some(Command::GetHook(res)));
        assert_eq!(result, "Some Title");

        let result = Command::from_str(".hook v5555");
        assert_result!(result, Some(Command::GetHookById(5555)));

        let result = Command::from_str(".hook v5555g");
        let result = assert_result!(ret => res, result, Some(Command::GetHook(res)));
        assert_eq!(result, "v5555g");

        let result = Command::from_str(".hook    gv5555");
        let result = assert_result!(ret => res, result, Some(Command::GetHook(res)));
        assert_eq!(result, "gv5555");
    }
}
