#[derive(Clone)]
pub struct PipeReader {}

impl PipeReader {
    pub fn try_clone(&self) -> std::io::Result<PipeReader> {
        Ok((*self).clone())
    }
}

impl From<PipeReader> for std::process::Stdio {
    fn from(_reader: PipeReader) -> Self {
        std::process::Stdio::null()
    }
}

impl std::io::Read for PipeReader {
    fn read(&mut self, _buf: &mut [u8]) -> std::io::Result<usize> {
        // TODO: implement
        Ok(0)
    }
}

#[derive(Clone)]
pub struct PipeWriter {}

impl PipeWriter {
    pub fn try_clone(&self) -> std::io::Result<PipeWriter> {
        Ok((*self).clone())
    }
}

impl From<PipeWriter> for std::process::Stdio {
    fn from(_writer: PipeWriter) -> Self {
        std::process::Stdio::null()
    }
}

impl std::io::Write for PipeWriter {
    fn write(&mut self, _buf: &[u8]) -> std::io::Result<usize> {
        // TODO: implement
        Ok(0)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

pub(crate) fn pipe() -> std::io::Result<(PipeReader, PipeWriter)> {
    Ok((PipeReader {}, PipeWriter {}))
}
