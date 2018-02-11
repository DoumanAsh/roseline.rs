extern crate actix;

extern crate db;

use self::actix::prelude::*;

use self::db::Db as InnerDb;
pub use self::db::models;

pub struct Db {
    inner: InnerDb,
}

impl Db {
    pub fn new() -> Self {
        Self {
            inner: InnerDb::new().expect("Actor DB to start")
        }
    }

    pub fn start_threaded(threads: usize) -> SyncAddress<Self> {
        SyncArbiter::start(threads, || Self::new())
    }
}

impl Actor for Db {
    type Context = SyncContext<Self>;
}

///Result of `GetVn` Command
pub struct VnData {
    pub data: models::Vn,
    pub hooks: Vec<models::Hook>
}

///Retrieves all information about VN
pub struct GetVnData(pub u64);
impl ResponseType for GetVnData {
    type Item = Option<VnData>;
    type Error = String;
}
impl Handler<GetVnData> for Db {
    type Result = MessageResult<GetVnData>;

    fn handle(&mut self, msg: GetVnData, _: &mut Self::Context) -> Self::Result {
        let vn = self.inner.get_vn(msg.0 as i64).map_err(|err| format!("{}", err))?;

        let vn = match vn {
            Some(vn) => vn,
            None => return Ok(None)
        };

        self.inner.get_hooks(&vn).map(|hooks| Some(VnData { data: vn, hooks }))
                                 .map_err(|err| format!("{}", err))
    }
}

///Retrieves VN
pub struct GetVn(pub u64);
impl ResponseType for GetVn {
    type Item = Option<models::Vn>;
    type Error = String;
}
impl Handler<GetVn> for Db {
    type Result = MessageResult<GetVn>;

    fn handle(&mut self, msg: GetVn, _: &mut Self::Context) -> Self::Result {
        self.inner.get_vn(msg.0 as i64).map_err(|err| format!("{}", err))
    }
}


///Retrieves hooks for VN.
pub struct GetHooks(pub models::Vn);
impl ResponseType for GetHooks {
    type Item = VnData;
    type Error = String;
}
impl Handler<GetHooks> for Db {
    type Result = MessageResult<GetHooks>;

    fn handle(&mut self, msg: GetHooks, _: &mut Self::Context) -> Self::Result {
        let vn = msg.0;
        self.inner.get_hooks(&vn).map(|hooks| VnData { data: vn, hooks })
                                 .map_err(|err| format!("{}", err))
    }
}

///Adds/update VN
pub struct PutVn {
    pub id: u64,
    pub title: String
}
impl ResponseType for PutVn {
    type Item = models::Vn;
    type Error = String;
}
impl Handler<PutVn> for Db {
    type Result = MessageResult<PutVn>;

    fn handle(&mut self, msg: PutVn, _: &mut Self::Context) -> Self::Result {
        let PutVn{id, title} = msg;
        self.inner.put_vn(id as i64, title).map_err(|err| format!("{}", err))
    }
}

///Adds/update hook for VN
pub struct PutHook {
    pub vn: models::Vn,
    pub version: String,
    pub code: String,
}
impl ResponseType for PutHook {
    type Item = models::HookView;
    type Error = String;
}
impl Handler<PutHook> for Db {
    type Result = MessageResult<PutHook>;

    fn handle(&mut self, msg: PutHook, _: &mut Self::Context) -> Self::Result {
        let PutHook{ vn, version, code } = msg;
        self.inner.put_hook(&vn, version, code).map_err(|err| format!("{}", err))
    }
}

///Search VNs by title in DB.
pub struct SearchVn(pub String);
impl ResponseType for SearchVn {
    type Item = Vec<models::Vn>;
    type Error = String;
}

impl Handler<SearchVn> for Db {
    type Result = MessageResult<SearchVn>;

    fn handle(&mut self, msg: SearchVn, _: &mut Self::Context) -> Self::Result {
        self.inner.search_vn(&msg.0).map_err(|err| format!("{}", err))
    }
}

///Deletes VN alongside all hooks
pub struct DelVnData(pub u64);
impl ResponseType for DelVnData {
    type Item = usize;
    type Error = String;
}

impl Handler<DelVnData> for Db {
    type Result = MessageResult<DelVnData>;

    fn handle(&mut self, msg: DelVnData, _: &mut Self::Context) -> Self::Result {
        self.inner.delete_vn(msg.0 as i64).map_err(|err| format!("{}", err))
    }
}

///Removes hook for VN
pub struct DelHook {
    pub vn: models::Vn,
    pub version: String,
}
impl ResponseType for DelHook {
    type Item = usize;
    type Error = String;
}
impl Handler<DelHook> for Db {
    type Result = MessageResult<DelHook>;

    fn handle(&mut self, msg: DelHook, _: &mut Self::Context) -> Self::Result {
        let DelHook{vn, version} = msg;
        self.inner.delete_hook(&vn, &version).map_err(|err| format!("{}", err))
    }
}
