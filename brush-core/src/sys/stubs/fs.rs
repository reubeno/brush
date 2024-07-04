#[cfg(not(unix))]
impl crate::sys::fs::PathExt for std::path::Path {
    fn readable(&self) -> bool {
        // TODO: implement
        true
    }

    fn writable(&self) -> bool {
        // TODO: implement
        true
    }

    fn executable(&self) -> bool {
        // TODO: implement
        true
    }

    fn exists_and_is_block_device(&self) -> bool {
        false
    }

    fn exists_and_is_char_device(&self) -> bool {
        false
    }

    fn exists_and_is_fifo(&self) -> bool {
        false
    }

    fn exists_and_is_socket(&self) -> bool {
        false
    }

    fn exists_and_is_setgid(&self) -> bool {
        false
    }

    fn exists_and_is_setuid(&self) -> bool {
        false
    }

    fn exists_and_is_sticky_bit(&self) -> bool {
        false
    }
}

pub(crate) trait StubMetadataExt {
    fn gid(&self) -> u32 {
        // TODO: implement
        0
    }

    fn uid(&self) -> u32 {
        // TODO: implement
        0
    }
}

impl StubMetadataExt for std::fs::Metadata {}
