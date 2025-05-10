use crate::{error, interfaces};

pub(crate) fn get_key_from_key_code(_key_code: &[u8]) -> Result<interfaces::Key, error::Error> {
    error::unimp("get_key_from_key_code")
}
