use std::path::Path;

pub(crate) trait PathExt {
    fn readable(&self) -> bool;
    fn writable(&self) -> bool;
    fn executable(&self) -> bool;
}

impl PathExt for Path {
    fn readable(&self) -> bool {
        nix::unistd::access(self, nix::unistd::AccessFlags::R_OK).is_ok()
    }

    fn writable(&self) -> bool {
        nix::unistd::access(self, nix::unistd::AccessFlags::W_OK).is_ok()
    }

    fn executable(&self) -> bool {
        nix::unistd::access(self, nix::unistd::AccessFlags::X_OK).is_ok()
    }
}
