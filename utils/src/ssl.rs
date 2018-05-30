#[cfg(all(unix, not(target_os = "macos"), not(target_os = "ios")))]
pub fn init() {
    extern crate openssl_probe;
    openssl_probe::init_ssl_cert_env_vars();
}

#[cfg(any(windows, target_os = "macos", target_os = "ios"))]
pub fn init() {
}
