pub(crate) fn get_hostname() -> std::io::Result<std::ffi::OsString> {
    crate::sys::hostname::get()
}

#[allow(clippy::unnecessary_wraps)]
pub(crate) fn get_all_services() -> Result<Vec<String>, crate::error::Error> {
    // TODO: implement for Unix-like systems, using getservent[_r]
    Ok(vec![])
}
