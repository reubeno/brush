pub(crate) fn get_hostname() -> std::io::Result<std::ffi::OsString> {
    Ok("".into())
}

#[allow(clippy::unnecessary_wraps)]
pub(crate) fn get_all_services() -> Result<Vec<String>, crate::error::Error> {
    Ok(vec![])
}
