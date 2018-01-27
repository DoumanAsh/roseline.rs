extern crate actix;

extern crate db;

use self::actix::prelude::*;

use self::db::Db as InnerDb;
use self::db::models;

//TODO: Arbiter doesn't clone whatsover so we can assume thread safety
//      But if we'll re-write bot to use actix, then Rc wouldn't be needed at all.
unsafe impl Send for Db {}
unsafe impl Sync for Db {}

pub struct Db {
    inner: InnerDb,
}

impl Db {
    pub fn new(threads: usize) -> SyncAddress<Db> {
        SyncArbiter::start(threads, || Self {
            inner: InnerDb::new().expect("Actor DB to start")
        })
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
