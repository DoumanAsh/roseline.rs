extern crate diesel;

pub mod create {
    pub const VNS: &'static str = "CREATE TABLE IF NOT EXISTS vns (
        id INTEGER,
        title TEXT NOT NULL,
        PRIMARY KEY (id)
)";
    pub const HOOKS: &'static str = "CREATE TABLE IF NOT EXISTS hooks (
        id INTEGER,
        vn_id INTEGER NOT NULL,
        version TEXT NOT NULL,
        code TEXT NOT NULL,
        PRIMARY KEY (id),
        FOREIGN KEY (vn_id) REFERENCES vns (id) ON DELETE CASCADE ON UPDATE NO ACTION
)";
}

table! {
    vns(id) {
        id -> BigInt,
        title -> Text,
    }
}

table! {
    hooks(id) {
        id -> BigInt,
        vn_id -> BigInt,
        version -> Text,
        code -> Text,
    }
}
