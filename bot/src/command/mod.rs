extern crate regex;
extern crate vndb;
extern crate actors;

mod args;

use self::args::shell_split;

use self::vndb::protocol::message::request::get::Type as VndbRequestType;
use ::fmt::Display;

pub const HELP: &'static str = "Available commands: .ping, .vn, .hook, .set_hook, .del_hook, .del_vn";
pub const SET_HOOK_USAGE: &'static str = "Usage: <title> <version> <code>";
pub const DEL_HOOK_USAGE: &'static str = "Usage: <title> <version>";

///Gets VN info
pub struct GetVn {
    pub title: String
}

#[derive(Clone)]
pub struct Ref {
    pub kind: VndbRequestType,
    pub id: u64,
    pub url: bool,
}

#[derive(Default)]
///Retrieves references to VNDB objects
pub struct Refs {
    pub refs: [Option<Ref>; 5]
}

///Responds with some text
pub struct Text(pub String);

impl Text {
    ///Returns generic error.
    pub fn error<T: Display>(error: T) -> Self {
        format!("ごめんなさい、エラー: {}", error).into()
    }
}

impl From<String> for Text {
    fn from(text: String) -> Self {
        Text(text)
    }
}

impl<'a> From<&'a str> for Text {
    fn from(text: &str) -> Self {
        Text(text.to_string())
    }
}

//.hook
pub struct GetHook {
    pub title: String
}

//.set_hook
pub struct SetHook {
    pub title: String,
    pub version: String,
    pub code: String,
}

//.del_hook
pub struct DelHook {
    pub title: String,
    pub version: String,
}

//.del_vn
pub struct DelVn {
    pub title: String
}

pub enum Command {
    Text(Text),
    GetVn(GetVn),
    GetHook(GetHook),
    SetHook(SetHook),
    DelHook(DelHook),
    DelVn(DelVn),
    Refs(Refs),
    Ignore(String),
    IgnoreList
}

pub fn extract_vndb_references(text: &str) -> Option<Refs> {
    lazy_static! {
        static ref EXTRACT_REFERENCE: regex::Regex = regex::Regex::new("(^|vndb\\.org/|\\s)([vcrpu])([0-9]+)").unwrap();
    }

    if !EXTRACT_REFERENCE.is_match(text) {
        return None;
    }

    let mut result = Refs::default();
    let mut result_idx = 0;

    for capture in EXTRACT_REFERENCE.captures_iter(text) {
        let kind = match capture.get(2) {
            Some(typ) => match typ.as_str() {
                "v" => VndbRequestType::vn(),
                "c" => VndbRequestType::character(),
                "r" => VndbRequestType::release(),
                "p" => VndbRequestType::producer(),
                "u" => VndbRequestType::user(),
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
        let url = match capture.get(1) {
            Some(prefix) => prefix.as_str().trim().is_empty(),
            None => true
        };

        result.refs[result_idx] = Some(Ref { kind, id, url });
        result_idx += 1;

        if result_idx > 4 {
            break;
        }
    }

    Some(result)
}

impl Command {
    pub fn from_str(text: &str) -> Option<Command> {
        lazy_static! {
            static ref EXTRACT_CMD: regex::Regex = regex::Regex::new("^\\s*\\.([^\\s]*)(\\s+(.+))*").unwrap();
        }

        const CMD_IDX: usize = 1;
        const ARG_IDX: usize = 3;

        if let Some(captures) = EXTRACT_CMD.captures(text) {
            let cmd = captures.get(CMD_IDX);
            let cmd = cmd.map(|cmd| cmd.as_str());

            match cmd {
                Some("ping") => Some(Command::Text("pong".into())),
                Some("help") => Some(Command::Text(HELP.into())),
                Some("ignore_list") => Some(Command::IgnoreList),
                Some("ignore") => match captures.get(ARG_IDX) {
                    Some(name) => Some(Command::Ignore(name.as_str().to_owned())),
                    None => Some(Command::Text("Who to ignore?".into()))
                },
                Some("vn") => match captures.get(ARG_IDX) {
                    Some(title) => Some(Command::GetVn(GetVn { title: title.as_str().to_owned() })),
                    None => Some(Command::Text("Which VN...?".into()))
                },
                Some("hook") => match captures.get(ARG_IDX) {
                    Some(arg) => Some(Command::GetHook(GetHook{ title: arg.as_str().trim().to_string()})),
                    None => Some(Command::Text("For which VN...?".into()))
                },
                Some("del_vn") => match captures.get(ARG_IDX) {
                    Some(arg) => Some(Command::DelVn(DelVn{ title: arg.as_str().trim().to_string()})),
                    None => Some(Command::Text("For which VN...?".into()))
                },
                Some("set_hook") => {
                    let arg = match captures.get(ARG_IDX) {
                        Some(arg) => arg,
                        None => return Some(Command::Text(SET_HOOK_USAGE.into())),
                    };

                    let args = match shell_split(arg.as_str()) {
                        Ok(args) => args,
                        Err(error) => return Some(Command::Text(Text::error(error)))
                    };

                    if args.len() != 3 {
                        return Some(Command::Text(format!("Invalid number of arguments {}. Expected 3", args.len()).into()))
                    }

                    let title = unsafe { args.get_unchecked(0).to_string() };
                    let version = unsafe { args.get_unchecked(1).to_string() };
                    let code = unsafe { args.get_unchecked(2).to_string() };

                    Some(Command::SetHook(SetHook {
                        title,
                        version,
                         code
                    }))
                },
                Some("del_hook") => {
                    let arg = match captures.get(ARG_IDX) {
                        Some(arg) => arg,
                        None => return Some(Command::Text(DEL_HOOK_USAGE.into())),
                    };

                    let args = match shell_split(arg.as_str()) {
                        Ok(args) => args,
                        Err(error) => return Some(Command::Text(Text::error(error))),
                    };

                    if args.len() != 2 {
                        return Some(Command::Text(format!("Invalid number of arguments {}. Expected 2", args.len()).into()))
                    }

                    let title = unsafe { args.get_unchecked(0).to_string() };
                    let version = unsafe { args.get_unchecked(1).to_string() };

                    Some(Command::DelHook(DelHook {
                        title,
                        version
                    }))
                },
                _ => None
            }
        }
        else if let Some(refs) = extract_vndb_references(text) {
            Some(Command::Refs(refs))
        }
        else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Command,
        Text,
        Refs,
        VndbRequestType,
        GetHook,
        SetHook,
        DelHook,
        DelVn,
        HELP,
        SET_HOOK_USAGE,
        DEL_HOOK_USAGE
    };

    #[test]
    fn should_cmd_pong() {
        match Command::from_str(" .ping") {
            Some(Command::Text(Text(text))) => assert_eq!(text, "pong"),
            _ => assert!(false)
        }

        match Command::from_str(".ping") {
            Some(Command::Text(Text(text))) => assert_eq!(text, "pong"),
            _ => assert!(false)
        }
    }

    #[test]
    fn should_cmd_help() {
        match Command::from_str(".help") {
            Some(Command::Text(Text(text))) => assert_eq!(text, HELP),
            _ => assert!(false)
        }

        match Command::from_str(" .help") {
            Some(Command::Text(Text(text))) => assert_eq!(text, HELP),
            _ => assert!(false)
        }
    }

    #[test]
    fn should_cmd_vn_ref() {
        let expected_refs = [
            Some((VndbRequestType::vn(), 1u64, true)),
            Some((VndbRequestType::vn(), 2u64, true)),
            Some((VndbRequestType::user(), 55u64, true)),
            Some((VndbRequestType::character(), 25u64, true)),
            None
        ];

        let result = match Command::from_str("v1 d2 v2 u55 c25") {
            Some(Command::Refs(Refs{refs})) => refs,
            _ => {
                assert!(false);
                return;
            }
        };

        for (idx, reference) in result.iter().enumerate() {
            let expected = unsafe { expected_refs.get_unchecked(idx) };
            assert_eq!(expected.is_some(), reference.is_some());

            if let Some(ref expected) = expected.as_ref() {
                let reference = reference.as_ref().unwrap();

                assert_eq!(reference.kind.short(), expected.0.short());
                assert_eq!(reference.id, expected.1);
                assert_eq!(reference.url, expected.2);
            }
        }

        let result = Command::from_str("g1 d2 g2");
        assert!(result.is_none());

        let result = Command::from_str("2v2");
        assert!(result.is_none());
    }

    #[test]
    fn should_cmd_vn_ref_url() {
        let result = match Command::from_str("Try this Vn https://vndb.org/v125").expect("Some result") {
            Command::Refs(Refs{refs}) => refs,
            _ => panic!("Unexpected result")
        };

        let first = unsafe { result.get_unchecked(0) };
        assert!(first.is_some());
        let first = first.as_ref().unwrap();
        assert_eq!(first.kind.short(), "v");
        assert_eq!(first.id, 125);
        assert_eq!(first.url, false);

        for reference in result.iter().skip(1) {
            assert!(reference.is_none());
        }

        let result = Command::from_str("Try this https://youtu.be/u7mDRIuXOUY");
        assert!(result.is_none());
    }

    #[test]
    fn should_cmd_get_hook() {
        match Command::from_str(".hook") {
            Some(Command::Text(Text(text))) => assert_eq!(text, "For which VN...?"),
            _ => panic!("Unexpected result for .hook")
        }

        match Command::from_str(".hook Some Title") {
            Some(Command::GetHook(GetHook{title})) => assert_eq!(title, "Some Title"),
            _ => panic!("Unexpected result for .hook")
        }
    }

    #[test]
    fn should_cmd_del_vn() {
        match Command::from_str(".del_vn") {
            Some(Command::Text(Text(text))) => assert_eq!(text, "For which VN...?"),
            _ => panic!("Unexpected result for .del_vn")
        }

        match Command::from_str(".del_vn vn5") {
            Some(Command::DelVn(DelVn{title})) => assert_eq!(title, "vn5"),
            _ => panic!("Unexpected result for .del_vn")
        }
    }

    #[test]
    fn should_cmd_set_hook() {
        match Command::from_str(".set_hook") {
            Some(Command::Text(Text(text))) => assert_eq!(text, SET_HOOK_USAGE),
            _ => panic!("Unexpected result for .set_hook")
        }

        match Command::from_str(".set_hook \"g'") {
            Some(Command::Text(Text(text))) => assert!(text.len() > 0),
            _ => panic!("Unexpected result for .set_hook")
        }

        match Command::from_str(".set_hook 1 2") {
            Some(Command::Text(Text(text))) => assert!(text.len() > 0),
            _ => panic!("Unexpected result for .set_hook")
        }

        match Command::from_str(".set_hook title version code") {
            Some(Command::SetHook(SetHook{title, version, code})) => {
                assert_eq!(title, "title");
                assert_eq!(version, "version");
                assert_eq!(code, "code");
            },
            _ => panic!("Unexpected result for .set_hook")
        }

        match Command::from_str(".set_hook 'title multi' version code") {
            Some(Command::SetHook(SetHook{title, version, code})) => {
                assert_eq!(title, "title multi");
                assert_eq!(version, "version");
                assert_eq!(code, "code");
            },
            _ => panic!("Unexpected result for .set_hook")
        }
    }
    #[test]
    fn should_cmd_del_hook() {
        match Command::from_str(".del_hook") {
            Some(Command::Text(Text(text))) => assert_eq!(text, DEL_HOOK_USAGE),
            _ => panic!("Unexpected result for .del_hook")
        }

        match Command::from_str(".del_hook 1") {
            Some(Command::Text(Text(text))) => assert!(text.len() > 0),
            _ => panic!("Unexpected result for .del_hook")
        }

        match Command::from_str(".del_hook 1 2 3") {
            Some(Command::Text(Text(text))) => assert!(text.len() > 0),
            _ => panic!("Unexpected result for .del_hook")
        }

        match Command::from_str(".del_hook title 'version complex'") {
            Some(Command::DelHook(DelHook{title, version})) => {
                assert_eq!(title, "title");
                assert_eq!(version, "version complex");
            },
            _ => panic!("Unexpected result for .del_hook")
        }
    }
}
