extern crate regex;
extern crate vndb;
extern crate actors;

mod args;

use self::args::shell_split;

use self::vndb::protocol::message::request::get::Type as VndbRequestType;
use ::fmt::Display;

const HELP: &'static str = "Available commands: .ping, .vn, .hook, .set_hook, .del_hook, .del_vn";
const SET_HOOK_USAGE: &'static str = "Usage: <title> <version> <code>";
const DEL_HOOK_USAGE: &'static str = "Usage: <title> <version>";

///Search for VN
pub struct SearchVn {
    pub title: String
}

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
    ///Returns error on too many search results
    pub fn too_many_vn_hits(num: usize, title: String) -> Self {
        lazy_static! {
            static ref RE: regex::Regex = regex::Regex::new("\\s+").unwrap();
        }

        let title = RE.replace_all(&title, "+");
        format!("There are too many hits='{}'. Try yourself -> https://vndb.org/v/all?sq={}", num, title).into()
    }

    pub fn better_vn_query(num: usize, title: &str) -> Self {
        format!("{} VNs have been found for query '{}'. Try a better query.", num, title).into()
    }

    ///Returns generic error.
    pub fn error<T: Display>(error: T) -> Self {
        format!("ごめんなさい、エラー: {}", error).into()
    }

    pub fn no_such_vn() -> Self {
        "No such VN could be found".into()
    }

    ///Returns error when VNDB does something bad.
    pub fn bad_vndb() -> Self {
        "Error with VNDB. Forgive me, I cannot execute your request".into()
    }

    ///Returns simplified text version of VN data with hooks.
    pub fn from_vn_data(vn: actors::db::VnData) -> Self {
        //TODO: maybe consider multi-line?
        match vn.hooks.len() {
            0 => format!("No hook exists for VN '{}'", vn.data.title).into(),
            1 => {
                let hook = unsafe { vn.hooks.get_unchecked(0) };
                format!("{} - {}", vn.data.title, hook.code).into()
            },
            _ => {
                let mut text = format!("{} - ", vn.data.title);

                for hook in vn.hooks {
                    text.push_str(&format!("{}: {} | ", hook.version, hook.code));
                }

                text[..text.len()-3].into()
            }
        }
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
pub struct GetHookByExactVndbTitle {
    pub title: String
}

pub struct GetHookByVndbTitle {
    pub title: String
}

pub struct GetHookByTitle {
    pub title: String
}

pub struct GetHookById {
    pub id: u64
}

//.set_hook
pub struct SetHookByExactVndbTitle {
    pub title: String,
    pub version: String,
    pub code: String,
}

pub struct SetHookByVndbTitle {
    pub title: String,
    pub version: String,
    pub code: String,
}

pub struct SetHookByTitle {
    pub title: String,
    pub version: String,
    pub code: String,
}

pub struct SetNewHook {
    pub id: u64,
    pub title: String,
    pub version: String,
    pub code: String,
}

pub struct SetVnHook {
    pub vn: actors::db::models::Vn,
    pub version: String,
    pub code: String,
}

pub struct SetHookById {
    pub id: u64,
    pub version: String,
    pub code: String,
}

pub struct SetHookByNewId {
    pub id: u64,
    pub version: String,
    pub code: String,
}

//.del_hook
pub struct DelHookByExactVndbTitle {
    pub title: String,
    pub version: String,
}

pub struct DelHookByVndbTitle {
    pub title: String,
    pub version: String,
}

pub struct DelHookByTitle {
    pub title: String,
    pub version: String,
}

pub struct DelHookById {
    pub id: u64,
    pub version: String,
}

pub struct DelVnHook {
    pub vn: actors::db::models::Vn,
    pub version: String,
}

//.del_vn
pub struct DelVnByExactVndbTitle {
    pub title: String
}

pub struct DelVnByVndbTitle {
    pub title: String
}

pub struct DelVnByTitle {
    pub title: String
}

pub struct DelVnById {
    pub id: u64
}

pub enum Command {
    Text(Text),
    GetVn(GetVn),
    GetHookByTitle(GetHookByTitle),
    GetHookById(GetHookById),
    SetHookByTitle(SetHookByTitle),
    SetHookById(SetHookById),
    DelHookByTitle(DelHookByTitle),
    DelHookById(DelHookById),
    DelVnByTitle(DelVnByTitle),
    DelVnById(DelVnById),
    Refs(Refs)
}

impl Command {
    pub fn from_str(text: &str) -> Option<Command> {
        lazy_static! {
            static ref EXTRACT_CMD: regex::Regex = regex::Regex::new("^\\s*\\.([^\\s]*)(\\s+(.+))*").unwrap();
            static ref EXTRACT_REFERENCE: regex::Regex = regex::Regex::new("(^|vndb\\.org/|\\s)([vcrpu])([0-9]+)").unwrap();
            static ref EXTRACT_VN_ID: regex::Regex = regex::Regex::new("^v([0-9]+)$").unwrap();
        }

        const CMD_IDX: usize = 1;
        const ARG_IDX: usize = 3;

        if let Some(captures) = EXTRACT_CMD.captures(text) {
            let cmd = captures.get(CMD_IDX);
            let cmd = cmd.map(|cmd| cmd.as_str());

            match cmd {
                Some("ping") => Some(Command::Text("pong".into())),
                Some("help") => Some(Command::Text(HELP.into())),
                Some("vn") => {
                    match captures.get(ARG_IDX) {
                        Some(title) => Some(Command::GetVn(GetVn { title: title.as_str().to_owned() })),
                        None => Some(Command::Text("Which VN...?".into()))
                    }
                },
                Some("hook") => {
                    let arg = match captures.get(ARG_IDX) {
                        Some(arg) => arg,
                        None => return Some(Command::Text("For which VN...?".into()))
                    };

                    let arg = arg.as_str().trim();
                    match EXTRACT_VN_ID.captures(arg).map(|cap| cap.get(1).map(|cap| cap.as_str()).map(|cap| cap.parse::<u64>())) {
                        Some(Some(Ok(capture))) => Some(Command::GetHookById(GetHookById { id: capture })),
                        _ => Some(Command::GetHookByTitle(GetHookByTitle { title: arg.to_owned()} )),
                    }
                },
                Some("del_vn") => {
                    let arg = match captures.get(ARG_IDX) {
                        Some(arg) => arg,
                        None => return Some(Command::Text("For which VN...?".into())),
                    };

                    let arg = arg.as_str().trim();
                    match EXTRACT_VN_ID.captures(arg).map(|cap| cap.get(1).map(|cap| cap.as_str()).map(|cap| cap.parse::<u64>())) {
                        Some(Some(Ok(capture))) => Some(Command::DelVnById(DelVnById { id: capture })),
                        _ => Some(Command::DelVnByTitle(DelVnByTitle { title: arg.to_owned()} )),
                    }
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

                    let title = unsafe { args.get_unchecked(0) };
                    let version = unsafe { args.get_unchecked(1).to_string() };
                    let code = unsafe { args.get_unchecked(2).to_string() };

                    match EXTRACT_VN_ID.captures(title).map(|cap| cap.get(1).map(|cap| cap.as_str()).map(|cap| cap.parse::<u64>())) {
                        Some(Some(Ok(capture))) => Some(Command::SetHookById( SetHookById {
                            id: capture,
                            version,
                            code
                        })),
                        _ => Some(Command::SetHookByTitle(SetHookByTitle {
                            title: title.to_string(),
                            version,
                            code
                        }))
                    }
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

                    let title = unsafe { args.get_unchecked(0) };
                    let version = unsafe { args.get_unchecked(1).to_string() };

                    match EXTRACT_VN_ID.captures(title).map(|cap| cap.get(1).map(|cap| cap.as_str()).map(|cap| cap.parse::<u64>())) {
                        Some(Some(Ok(capture))) => Some(Command::DelHookById(DelHookById { id: capture, version })),
                        _ => Some(Command::DelHookByTitle(DelHookByTitle { title: title.to_string(), version }))
                    }
                },
                _ => None
            }
        }
        else if EXTRACT_REFERENCE.is_match(text) {
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

            Some(Command::Refs(result))
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
        GetHookByTitle,
        GetHookById,
        DelVnByTitle,
        DelVnById,
        SetHookById,
        SetHookByTitle,
        VndbRequestType,
        DelHookById,
        DelHookByTitle,
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
    fn should_valid_too_many_vn_hits() {
        let expected = "There are too many hits='7'. Try yourself -> https://vndb.org/v/all?sq=Aoi+Tori";
        let result = Text::too_many_vn_hits(7, "Aoi  \tTori".to_string());

        assert_eq!(result.0, expected);
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
            Some(Command::GetHookByTitle(GetHookByTitle{title})) => assert_eq!(title, "Some Title"),
            _ => panic!("Unexpected result for .hook")
        }

        match Command::from_str(".hook v5555") {
            Some(Command::GetHookById(GetHookById{id})) => assert_eq!(id, 5555),
            _ => panic!("Unexpected result for .hook")
        }

        match Command::from_str(".hook v5555g") {
            Some(Command::GetHookByTitle(GetHookByTitle{title})) => assert_eq!(title, "v5555g"),
            _ => panic!("Unexpected result for .hook")
        }

        match Command::from_str(".hook    gv5555") {
            Some(Command::GetHookByTitle(GetHookByTitle{title})) => assert_eq!(title, "gv5555"),
            _ => panic!("Unexpected result for .hook")
        }
    }

    #[test]
    fn should_cmd_del_vn() {
        match Command::from_str(".del_vn") {
            Some(Command::Text(Text(text))) => assert_eq!(text, "For which VN...?"),
            _ => panic!("Unexpected result for .del_vn")
        }

        match Command::from_str(".del_vn v5") {
            Some(Command::DelVnById(DelVnById{id})) => assert_eq!(id, 5),
            _ => panic!("Unexpected result for .del_vn")
        }

        match Command::from_str(".del_vn vn5") {
            Some(Command::DelVnByTitle(DelVnByTitle{title})) => assert_eq!(title, "vn5"),
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

        match Command::from_str(".set_hook v5 version code") {
            Some(Command::SetHookById(SetHookById{id, version, code})) => {
                assert_eq!(id, 5);
                assert_eq!(version, "version");
                assert_eq!(code, "code");
            },
            _ => panic!("Unexpected result for .set_hook")
        }

        match Command::from_str(".set_hook title version code") {
            Some(Command::SetHookByTitle(SetHookByTitle{title, version, code})) => {
                assert_eq!(title, "title");
                assert_eq!(version, "version");
                assert_eq!(code, "code");
            },
            _ => panic!("Unexpected result for .set_hook")
        }

        match Command::from_str(".set_hook 'title multi' version code") {
            Some(Command::SetHookByTitle(SetHookByTitle{title, version, code})) => {
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

        match Command::from_str(".del_hook v52 version") {
            Some(Command::DelHookById(DelHookById{id, version})) => {
                assert_eq!(id, 52);
                assert_eq!(version, "version");
            },
            _ => panic!("Unexpected result for .del_hook")
        }

        match Command::from_str(".del_hook title 'version complex'") {
            Some(Command::DelHookByTitle(DelHookByTitle{title, version})) => {
                assert_eq!(title, "title");
                assert_eq!(version, "version complex");
            },
            _ => panic!("Unexpected result for .del_hook")
        }
    }
}
