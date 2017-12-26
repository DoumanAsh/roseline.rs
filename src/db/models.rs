use db::schema::{vns, hooks};

#[derive(Identifiable, Insertable, Queryable, Debug)]
#[table_name = "vns"]
pub struct Vn {
    pub id: i32,
    pub title: String
}

#[derive(Identifiable, Associations, Insertable, Queryable, Debug)]
#[belongs_to(Vn)]
#[table_name = "hooks"]
pub struct Hook {
    id: i32,
    pub vn_id: i32,
    pub version: String,
    pub code: String
}
