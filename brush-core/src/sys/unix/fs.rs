use std::os::unix::fs::{FileTypeExt, MetadataExt};
use std::path::Path;

impl crate::sys::fs::PathExt for Path {
    fn readable(&self) -> bool {
        nix::unistd::access(self, nix::unistd::AccessFlags::R_OK).is_ok()
    }

    fn writable(&self) -> bool {
        nix::unistd::access(self, nix::unistd::AccessFlags::W_OK).is_ok()
    }

    fn executable(&self) -> bool {
        nix::unistd::access(self, nix::unistd::AccessFlags::X_OK).is_ok()
    }

    fn exists_and_is_block_device(&self) -> bool {
        try_get_file_type(self).map_or(false, |ft| ft.is_block_device())
    }

    fn exists_and_is_char_device(&self) -> bool {
        try_get_file_type(self).map_or(false, |ft| ft.is_char_device())
    }

    fn exists_and_is_fifo(&self) -> bool {
        try_get_file_type(self).map_or(false, |ft: std::fs::FileType| ft.is_fifo())
    }

    fn exists_and_is_socket(&self) -> bool {
        try_get_file_type(self).map_or(false, |ft| ft.is_socket())
    }

    fn exists_and_is_setgid(&self) -> bool {
        const S_ISGID: u32 = 0o2000;
        let file_mode = try_get_file_mode(self);
        file_mode.map_or(false, |mode| mode & S_ISGID != 0)
    }

    fn exists_and_is_setuid(&self) -> bool {
        const S_ISUID: u32 = 0o4000;
        let file_mode = try_get_file_mode(self);
        file_mode.map_or(false, |mode| mode & S_ISUID != 0)
    }

    fn exists_and_is_sticky_bit(&self) -> bool {
        const S_ISVTX: u32 = 0o1000;
        let file_mode = try_get_file_mode(self);
        file_mode.map_or(false, |mode| mode & S_ISVTX != 0)
    }
}

fn try_get_file_type(path: &Path) -> Option<std::fs::FileType> {
    path.metadata().map(|metadata| metadata.file_type()).ok()
}

fn try_get_file_mode(path: &Path) -> Option<u32> {
    path.metadata().map(|metadata| metadata.mode()).ok()
}
