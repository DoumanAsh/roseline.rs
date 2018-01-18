use ::schema::{vns, hooks};

#[derive(Identifiable, Insertable, Queryable, Debug)]
#[table_name = "vns"]
pub struct Vn {
    pub id: i64,
    pub title: String
}

#[derive(Identifiable, Associations, Insertable, Queryable, Debug)]
#[belongs_to(Vn)]
#[table_name = "hooks"]
pub struct Hook {
    pub id: i64,
    pub vn_id: i64,
    pub version: String,
    pub code: String
}

#[derive(Associations, Insertable, Queryable, Debug)]
#[belongs_to(Vn)]
#[table_name = "hooks"]
pub struct HookView {
    pub vn_id: i64,
    pub version: String,
    pub code: String
}
