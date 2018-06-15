extern crate lazy_panic;
extern crate cute_log;

///Initializes logging facilities
pub fn init() {
    set_panic_message!(lazy_panic::formatter::Simple);
    cute_log::init().expect("To initialize log");
}
