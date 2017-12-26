extern crate diesel;

pub mod schema;
pub mod models;

use self::diesel::{
    sql_query,
    Connection
};
use self::diesel::sqlite::SqliteConnection;

pub use self::diesel::{
    result,
    RunQueryDsl,
    QueryDsl,
    ExpressionMethods,
    OptionalExtension,
    BelongingToDsl
};

use ::utils::ResultExt;

use ::rc::Rc;

#[derive(Clone)]
pub struct Db {
    inner: Rc<SqliteConnection>
}

impl Db {
    pub fn new() -> Result<Self, String> {
        let conn = SqliteConnection::establish("./roseline.db").format_err("To start DB")?;
        sql_query(schema::create::VNS).execute(&conn).format_err("create table")?;
        sql_query(schema::create::HOOKS).execute(&conn).format_err("create table")?;

        Ok(Self {
            inner: Rc::new(conn)
        })
    }

    pub fn get_vn(&self, id: i32) -> result::QueryResult<Option<models::Vn>> {
        schema::vns::table.find(id).first::<models::Vn>(&*self.inner).optional()
    }

    pub fn get_hooks(&self, vn: &models::Vn) -> result::QueryResult<Vec<models::Hook>> {
        models::Hook::belonging_to(vn).get_results(&*self.inner)
    }

    #[inline]
    ///Retrieves VN table
    pub fn vns() -> schema::vns::table {
        schema::vns::table
    }

    #[inline]
    ///Retrieves Hook table
    pub fn hooks() -> schema::hooks::table {
        schema::hooks::table
    }
}

impl ::std::ops::Deref for Db {
    type Target = SqliteConnection;

    fn deref(&self) -> &SqliteConnection {
        &self.inner
    }
}
