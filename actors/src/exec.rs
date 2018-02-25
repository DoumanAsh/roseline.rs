extern crate actix;
extern crate futures;

use ::vndb;
use ::db;

use ::mem;

use vndb::protocol::message::request::get::Type as VndbRequestType;
use self::futures::{future, Future};
use self::actix::prelude::*;

fn parse_vndb_ref(text: &str) -> Option<(VndbRequestType, u64)> {
    let mut text = text.chars();

    let kind = match text.next() {
        Some('v') => VndbRequestType::vn(),
        Some('c') => VndbRequestType::character(),
        Some('r') => VndbRequestType::release(),
        Some('p') => VndbRequestType::producer(),
        Some('u') => VndbRequestType::user(),
        _ => return None
    };

    //as_str of Chars iterator returs slice of remaining chars
    let text = text.as_str();

    let id = match text.parse::<u64>() {
        Ok(result) if result > 0 => result,
        _ => return None
    };

    Some((kind, id))
}

///Performs execution of various commands
///that involves VNDB or DB
pub struct Executor {
    vndb: Addr<Unsync, vndb::Vndb>,
    db: Addr<Syn, db::Db>,
}

impl Executor {
    pub fn new(vndb: Addr<Unsync, vndb::Vndb>, db: Addr<Syn, db::Db>) -> Self {
        Self {
            vndb,
            db
        }
    }

    ///Starts Executor with default vndb and db actors
    pub fn default_threads(threads: usize) -> Self {
        let db: Addr<Syn, _> = db::Db::start_threaded(threads);
        let vndb: Addr<Unsync, _> = Supervisor::start(|_| vndb::Vndb::new());

        Self {
            vndb,
            db
        }
    }
}

impl Actor for Executor {
    type Context = Context<Self>;
}

///Possible errors
pub enum ResponseError {
    ///Unable to send VNDB request
    BadVndb,
    ///Received bad response from VNDB.
    BadVndbResponse,
    ///Too many VNs have been found
    TooMany(usize, String),
    ///Too many matches in DB
    TooManyDb(usize),
    ///Couldn't find such VN.
    UnknownVn,
    ///Invalid VNDB object ID.
    InvalidVnId(VndbRequestType, u64),
    ///Internal error that is not supposed to happen
    Internal(String)
}

impl ::fmt::Display for ResponseError {
    fn fmt(&self, f: &mut ::fmt::Formatter) -> ::fmt::Result {
        match self {
            &ResponseError::BadVndb => write!(f, "Error with VNDB. Forgive me, I cannot execute your request"),
            &ResponseError::BadVndbResponse => write!(f, "Bad VNDB response. Forgive me."),
            &ResponseError::UnknownVn => write!(f, "No such VN could be found."),
            &ResponseError::TooMany(ref num, ref title) => write!(f, "There are too many hits>='{}'. Try yourself -> https://vndb.org/v/all?sq={}", num, title.replace(" ", "+")),
            &ResponseError::TooManyDb(ref num) => write!(f, "Found '{}' matches in DB. Try a better query.", num),
            &ResponseError::InvalidVnId(ref kind, ref num) => write!(f, "{}{} is not an VN ID", kind.short(), num),
            &ResponseError::Internal(ref error) => write!(f, "ごめんなさい、エラー: {}", error)
        }
    }
}

macro_rules! try_vndb_response {
    ($resp:expr, $return_value:expr) => {{ match $resp {
        Ok(results) => results,
        Err(error) => {
            warn!("Error processing VNDB request: {}", error);
            return $return_value
        }
    }}};
    (Err $resp:expr) => {{
        try_vndb_response!($resp, Err(ResponseError::BadVndb))
    }};
    ($resp:expr) => {{
        try_vndb_response!($resp, Box::new(future::err(ResponseError::BadVndb)))
    }};
}

macro_rules! try_vndb_results {
    ($resp:expr, $return_value:expr) => {{ match $resp {
        vndb::Response::Results(result) => result,
        other => {
            error!("Unexpected VNDB response on get: {:?}", other);
            return $return_value
        }
    }}};
    ($resp:expr) => {{
        try_vndb_results!($resp, Box::new(future::err(ResponseError::BadVndbResponse)))
    }};
    (Err $resp:expr) => {{
        try_vndb_results!($resp, Err(ResponseError::BadVndbResponse))
    }}
}

macro_rules! try_vndb_result_type {
    ($resp:expr, $return_value:expr) => {{ match $resp {
        Ok(result) => result,
        Err(error) => {
            error!("Unexpected VNDB response type: {:?}", error);
            return $return_value
        }
    }}};
    ($resp:expr) => {{
        try_vndb_result_type!($resp, Box::new(future::err(ResponseError::BadVndbResponse)))
    }};
    (Err $resp:expr) => {{
        try_vndb_result_type!($resp, Err(ResponseError::BadVndbResponse))
    }}
}

///Get VN by ID
pub struct GetVn {
    id: u64
}
impl GetVn {
    pub fn new(id: u64) -> Self {
        Self {
            id
        }
    }
}
impl Message for GetVn {
    type Result = Result<vndb::response::results::Vn, ResponseError>;
}
type GetVnResponseFuture = Box<Future<Item=vndb::response::results::Vn, Error=ResponseError>>;
impl Handler<GetVn> for Executor {
    type Result = GetVnResponseFuture;

    fn handle(&mut self, msg: GetVn, _ctx: &mut Self::Context) -> Self::Result {
        let GetVn {id} = msg;

        let get_vn = vndb::Get::vn_by_id(id);
        let get_vn = self.vndb.send(get_vn.into()).map_err(|error| {
            error!("Error processing GetVn: {}", error);
            ResponseError::Internal(format!("{}", error))
        }).and_then(|result| {
            let result = try_vndb_response!(Err result);
            let result = try_vndb_results!(Err result);
            let mut result = try_vndb_result_type!(Err result.vn());
            let mut items = result.items.drain(..);

            match items.next() {
                Some(vn) => Ok(vn),
                None => Err(ResponseError::UnknownVn)
            }
        });

        Box::new(get_vn)
    }
}
///Get VN by ID in DB
pub struct GetVnDb(pub GetVn);
impl GetVnDb {
    pub fn new(id: u64) -> Self {
        GetVnDb(GetVn::new(id))
    }
}
impl Message for GetVnDb {
    type Result = Result<Option<db::models::Vn>, ResponseError>;
}
type GetVnDbResponseFuture = Box<Future<Item=Option<db::models::Vn>, Error=ResponseError>>;
impl Handler<GetVnDb> for Executor {
    type Result = GetVnDbResponseFuture;

    fn handle(&mut self, msg: GetVnDb, _ctx: &mut Self::Context) -> Self::Result {
        let id = msg.0.id;

        let get_vn = db::GetVn(id);
        let get_vn = self.db.send(get_vn).map_err(|error| {
            error!("Error processing GetVnDb: {}", error);
            ResponseError::Internal(format!("{}", error))
        }).and_then(move |result| match result {
            Ok(result) => Ok(result),
            Err(error) => Err(ResponseError::Internal(error))
        });

        Box::new(get_vn)
    }
}

///Get VN by title
pub struct FindVn {
    title: String
}
impl FindVn {
    pub fn new(title: String) -> Self {
        Self {
            title
        }
    }
}
impl Message for FindVn {
    type Result = Result<vndb::response::results::Vn, ResponseError>;
}
type FindVnResponseFuture = Box<Future<Item=vndb::response::results::Vn, Error=ResponseError>>;
impl Handler<FindVn> for Executor {
    type Result = FindVnResponseFuture;

    fn handle(&mut self, msg: FindVn, _ctx: &mut Self::Context) -> Self::Result {
        let FindVn {title} = msg;

        let get_vn = vndb::Get::vn_by_exact_title(&title);
        let vndb = self.vndb.clone();
        let get_vn = self.vndb.send(get_vn.into()).map_err(|error| {
            error!("Error processing FindVn: {}", error);
            ResponseError::Internal(format!("{}", error))
        }).and_then(move |result| -> FindVnResponseFuture {
            let result = try_vndb_response!(result);
            let result = try_vndb_results!(result);
            let mut result = try_vndb_result_type!(result.vn());

            match result.items.len() {
                1 => Box::new(future::ok(result.items.drain(..).next().unwrap())),
                _ => {
                    let search_vn = vndb::Get::vn_by_title(&title);
                    let search_vn = vndb.send(search_vn.into()).map_err(|error| {
                        error!("Error processing FindVn: {}", error);
                        ResponseError::Internal(format!("{}", error))
                    }).and_then(|result| {
                        let result = try_vndb_response!(result);
                        let result = try_vndb_results!(result);
                        let mut result = try_vndb_result_type!(result.vn());

                        let result = match result.items.len() {
                            0 => return Box::new(future::err(ResponseError::UnknownVn)),
                            1 => result.items.drain(..).next().unwrap(),
                            num => {
                                return Box::new(future::err(ResponseError::TooMany(num, title)));
                            }
                        };

                        Box::new(future::ok(result))
                    });

                    Box::new(search_vn)
                }
            }
        });

        Box::new(get_vn)
    }
}
///Get VN by title
pub struct FindVnDb(pub FindVn);
impl FindVnDb {
    pub fn new(title: String) -> Self {
        FindVnDb(FindVn::new(title))
    }
}
impl Message for FindVnDb {
    type Result = Result<Option<db::models::Vn>, ResponseError>;
}
type FindVnDbResponseFuture = Box<Future<Item=Option<db::models::Vn>, Error=ResponseError>>;
impl Handler<FindVnDb> for Executor {
    type Result = FindVnDbResponseFuture;

    fn handle(&mut self, msg: FindVnDb, _ctx: &mut Self::Context) -> Self::Result {
        let title = msg.0.title;

        let search_vn = db::SearchVn(title);
        let search_vn = self.db.send(search_vn).map_err(|error| {
            error!("Error processing FindVnDb: {}", error);
            ResponseError::Internal(format!("{}", error))
        }).and_then(move |result| {
            let mut vns = match result {
                Ok(vns) => vns,
                Err(error) => return Err(ResponseError::Internal(error)),
            };

            match vns.len() {
                0 => Ok(None),
                1 => Ok(vns.drain(..).next()),
                num => Err(ResponseError::TooManyDb(num))
            }
        });

        Box::new(search_vn)
    }
}

pub struct GetHook(pub String);
impl Message for GetHook {
    type Result = Result<db::VnData, ResponseError>;
}
type GetHookResponseFuture = Box<Future<Item=db::VnData, Error=ResponseError>>;
impl Handler<GetHook> for Executor {
    type Result = GetHookResponseFuture;

    fn handle(&mut self, msg: GetHook, ctx: &mut Self::Context) -> Self::Result {
        let title = msg.0;

        let this: &'static mut Self = unsafe { mem::transmute(self) };
        let ctx: &'static mut Self::Context = unsafe { mem::transmute(ctx) };

        let get_vn = match parse_vndb_ref(&title) {
            Some((kind, id)) => {
                if kind.short() != "v" {
                    return Box::new(future::err(ResponseError::InvalidVnId(kind, id)));
                }
                let get_vn = db::GetVnData(id);
                let get_vn = this.db.send(get_vn).map_err(|error| {
                    error!("Error processing GetVnData: {}", error);
                    ResponseError::Internal(format!("{}", error))
                }).and_then(|result| match result {
                    Ok(Some(result)) => Ok(result),
                    Ok(None) => Err(ResponseError::UnknownVn),
                    Err(error) => Err(ResponseError::Internal(error))
                }).then(|result| result);

                future::Either::A(get_vn)
            },
            None => {
                let get_vn = FindVnDb::new(title.clone());
                let get_vn = this.handle(get_vn, ctx).and_then(move |result| match result {
                    Some(vn) => future::Either::A(future::ok((vn, this))),
                    None => {
                        let get_vn = FindVn::new(title);
                        let get_vn = this.handle(get_vn, ctx).and_then(move |result| {
                            let get_vn = GetVnDb::new(result.id);
                            let get_vn = this.handle(get_vn, ctx).and_then(|result| match result {
                                Some(vn) => Ok((vn, this)),
                                None => Err(ResponseError::UnknownVn)
                            });

                            get_vn
                        });

                        future::Either::B(get_vn)
                    }
                }).and_then(|(vn, this)| {
                    let get_hooks = db::GetHooks(vn);
                    let get_hooks = this.db.send(get_hooks).map_err(|error| {
                        error!("Error processing GetHooks: {}", error);
                        ResponseError::Internal(format!("{}", error))
                    }).map(|result| result.map_err(|error| {
                        error!("Error processing getting hooks: {}", error);
                        ResponseError::Internal(format!("{}", error))
                    })).flatten();

                    Box::new(get_hooks)
                });

                future::Either::B(get_vn)
            }
        };

        Box::new(get_vn)
    }
}

pub struct SetHook {
    title: String,
    version: String,
    code: String
}
impl SetHook {
    pub fn new(title: String, version: String, code: String) -> Self {
        Self {
            title,
            version,
            code
        }
    }
}
impl Message for SetHook {
    type Result = Result<db::models::HookView, ResponseError>;
}
type SetHookResponseFuture = Box<Future<Item=db::models::HookView, Error=ResponseError>>;
impl Handler<SetHook> for Executor {
    type Result = SetHookResponseFuture;

    fn handle(&mut self, msg: SetHook, ctx: &mut Self::Context) -> Self::Result {
        let SetHook{title, version, code} = msg;

        let this: &'static mut Self = unsafe { mem::transmute(self) };
        let ctx: &'static mut Self::Context = unsafe { mem::transmute(ctx) };

        let get_vn = match parse_vndb_ref(&title) {
            Some((kind, id)) => {
                if kind.short() != "v" {
                    return Box::new(future::err(ResponseError::InvalidVnId(kind, id)));
                }
                let get_vn = GetVnDb::new(id);
                let get_vn = this.handle(get_vn, ctx).and_then(move |result| match result {
                    Some(vn) => future::Either::A(future::ok((vn, this))),
                    None => {
                        let get_vn = GetVn::new(id);
                        let get_vn = this.handle(get_vn, ctx).and_then(move |vn| {
                            let put_vn = db::PutVn { id: vn.id, title: vn.title.unwrap() };
                            this.db.send(put_vn).map_err(|error| {
                                error!("Error processing PutVn: {}", error);
                                ResponseError::Internal(format!("{}", error))
                            }).map(|result| result.map_err(|error| {
                                error!("Error performing PutVn: {}", error);
                                ResponseError::Internal(format!("{}", error))
                            })).flatten().map(|vn| (vn, this))
                        });
                        future::Either::B(get_vn)
                    }
                });

                future::Either::A(get_vn)
            },
            None => {
                let get_vn = FindVn::new(title);
                let get_vn = this.handle(get_vn, ctx).and_then(move |vn| {
                    let put_vn = db::PutVn { id: vn.id, title: vn.title.unwrap() };
                    this.db.send(put_vn).map_err(|error| {
                        error!("Error processing PutVn: {}", error);
                        ResponseError::Internal(format!("{}", error))
                    }).map(|result| result.map_err(|error| {
                        error!("Error performing PutVn: {}", error);
                        ResponseError::Internal(format!("{}", error))
                    })).flatten().map(|vn| (vn, this))
                });

                future::Either::B(get_vn)
            }
        }.and_then(|(vn, this)| -> SetHookResponseFuture {
            let put_hook = db::PutHook { vn, version, code };
            let put_hook = this.db.send(put_hook).map_err(|error| {
                error!("Error processing PutHook: {}", error);
                ResponseError::Internal(format!("{}", error))
            }).map(|result| result.map_err(|error| {
                error!("DB error: {}", error);
                ResponseError::Internal(error)
            })).flatten();

            Box::new(put_hook)
        });

        Box::new(get_vn)
    }
}

pub struct DelHook {
    title: String,
    version: String
}
impl DelHook {
    pub fn new(title: String, version: String) -> Self {
        Self {
            title,
            version
        }
    }
}
impl Message for DelHook {
    type Result = Result<usize, ResponseError>;
}
type DelHookResponseFuture = Box<Future<Item=usize, Error=ResponseError>>;
impl Handler<DelHook> for Executor {
    type Result = DelHookResponseFuture;

    fn handle(&mut self, msg: DelHook, ctx: &mut Self::Context) -> Self::Result {
        let DelHook{title, version} = msg;

        let this: &'static mut Self = unsafe { mem::transmute(self) };
        let ctx: &'static mut Self::Context = unsafe { mem::transmute(ctx) };

        let get_vn = match parse_vndb_ref(&title) {
            Some((kind, id)) => {
                if kind.short() != "v" {
                    return Box::new(future::err(ResponseError::InvalidVnId(kind, id)));
                }
                let get_vn = db::GetVn(id);
                let get_vn = this.db.send(get_vn).map_err(|error| {
                    error!("Error processing GetVn: {}", error);
                    ResponseError::Internal(format!("{}", error))
                }).and_then(|result| match result {
                    Ok(Some(result)) => Ok((result, this)),
                    Ok(None) => Err(ResponseError::UnknownVn),
                    Err(error) => Err(ResponseError::Internal(error))
                }).then(|result| result);

                future::Either::A(get_vn)
            },
            None => {
                let get_vn = FindVnDb::new(title.clone());
                let get_vn = this.handle(get_vn, ctx).and_then(move |result| match result {
                    Some(vn) => future::Either::A(future::ok((vn, this))),
                    None => {
                        let get_vn = FindVn::new(title);
                        let get_vn = this.handle(get_vn, ctx).and_then(move |result| {
                            let get_vn = GetVnDb::new(result.id);
                            let get_vn = this.handle(get_vn, ctx).and_then(|result| match result {
                                Some(vn) => Ok((vn, this)),
                                None => Err(ResponseError::UnknownVn)
                            });

                            get_vn
                        });

                        future::Either::B(get_vn)
                    }
                });

                future::Either::B(get_vn)
            }
        }.and_then(move |(vn, this)| {
            let del_hook = db::DelHook { vn, version };
            this.db.send(del_hook).map_err(|error| {
                error!("Error processing DelHook: {}", error);
                ResponseError::Internal(format!("{}", error))
            }).and_then(|result| match result {
                Ok(num) => Ok(num),
                Err(error) => Err(ResponseError::Internal(error))
            })
        });

        Box::new(get_vn)
    }
}

pub struct DelVn(pub String);
impl Message for DelVn {
    type Result = Result<usize, ResponseError>;
}
type DelVnResponseFuture = Box<Future<Item=usize, Error=ResponseError>>;
impl Handler<DelVn> for Executor {
    type Result = DelVnResponseFuture;

    fn handle(&mut self, msg: DelVn, ctx: &mut Self::Context) -> Self::Result {
        let title = msg.0;

        let this: &'static mut Self = unsafe { mem::transmute(self) };
        let ctx: &'static mut Self::Context = unsafe { mem::transmute(ctx) };

        let del_vn = match parse_vndb_ref(&title) {
            Some((kind, id)) => {
                if kind.short() != "v" {
                    return Box::new(future::err(ResponseError::InvalidVnId(kind, id)));
                }

                future::Either::A(future::ok((id as u64, this)))
            },
            None => {
                let get_vn = FindVnDb::new(title.clone());
                let get_vn = this.handle(get_vn, ctx).and_then(move |result| match result {
                    Some(vn) => future::Either::A(future::ok((vn.id as u64, this))),
                    None => {
                        let get_vn = FindVn::new(title);
                        let get_vn = this.handle(get_vn, ctx).and_then(move |result| Ok((result.id, this)));

                        future::Either::B(get_vn)
                    }
                });

                future::Either::B(get_vn)
            }
        }.and_then(|(id, this)| {
            let del_vn = db::DelVnData(id);
            this.db.send(del_vn).map_err(|error| {
                error!("Error processing DelVnData: {}", error);
                ResponseError::Internal(format!("{}", error))
            }).and_then(|result| match result {
                Ok(num) => Ok(num),
                Err(error) => Err(ResponseError::Internal(error))
            })
        });

        Box::new(del_vn)
    }
}

pub struct GetVndbObject {
    id: u64,
    kind: VndbRequestType
}
impl GetVndbObject {
    pub fn new(id: u64, kind: VndbRequestType) -> Self {
        Self {
            id,
            kind
        }
    }
}
impl Message for GetVndbObject {
    type Result = Result<vndb::response::Results, ResponseError>;
}
type GetVndbObjectResponseFuture = Box<Future<Item=vndb::response::Results, Error=ResponseError>>;
impl Handler<GetVndbObject> for Executor {
    type Result = GetVndbObjectResponseFuture;

    fn handle(&mut self, msg: GetVndbObject, _ctx: &mut Self::Context) -> Self::Result {
        let GetVndbObject {id, kind} = msg;

        let get_ref = vndb::Get::get_by_id(kind.clone(), id);
        let get_ref = self.vndb.send(get_ref.into()).map_err(|error| {
            error!("Error processing GetVndbObject: {}", error);
            ResponseError::Internal(format!("{}", error))
        }).and_then(|result| {
            let result = try_vndb_response!(Err result);
            let result = try_vndb_results!(Err result);

            Ok(result)
        });

        Box::new(get_ref)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        parse_vndb_ref
    };

    #[test]
    fn should_parse_vndb_ref() {
        let result = parse_vndb_ref("v555").expect("To parse");
        assert_eq!(result.1, 555);
        assert_eq!(result.0.short(), "v");

        let result = parse_vndb_ref("c123").expect("To parse");
        assert_eq!(result.1, 123);
        assert_eq!(result.0.short(), "c");
    }

    #[test]
    fn should_not_parse_vndb_ref() {
        let result = parse_vndb_ref("555");
        assert!(result.is_none());

        let result = parse_vndb_ref("g555");
        assert!(result.is_none());

        let result = parse_vndb_ref("v");
        assert!(result.is_none());

        let result = parse_vndb_ref("v0");
        assert!(result.is_none());
    }
}
