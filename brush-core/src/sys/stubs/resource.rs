use crate::error;

#[expect(clippy::unnecessary_wraps)]
pub(crate) fn get_self_user_and_system_time()
-> Result<(std::time::Duration, std::time::Duration), error::Error> {
    Ok((std::time::Duration::ZERO, std::time::Duration::ZERO))
}

#[expect(clippy::unnecessary_wraps)]
pub(crate) fn get_children_user_and_system_time()
-> Result<(std::time::Duration, std::time::Duration), error::Error> {
    Ok((std::time::Duration::ZERO, std::time::Duration::ZERO))
}
