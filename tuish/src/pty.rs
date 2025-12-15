//! PTY (pseudo-terminal) abstraction for tuish.
//!
//! This module provides safe-ish wrappers around PTY creation and management,
//! including VT100 parsing and I/O channel setup.

use std::io::{BufWriter, Read, Write};
use std::os::fd::FromRawFd;
use std::sync::{Arc, RwLock};

use bytes::Bytes;
use tokio::sync::mpsc::{Sender, channel};

/// Encapsulates a PTY instance with associated parser and I/O channels.
///
/// This struct owns the PTY file descriptors and manages background threads
/// for reading/writing PTY data.
pub struct Pty {
    /// VT100 parser for terminal emulation
    parser: Arc<RwLock<vt100::Parser>>,
    /// Channel sender for writing to the PTY
    writer: Sender<Bytes>,
    /// Master file descriptor for ioctl operations
    master_fd: libc::c_int,
    /// PTY slave file for stdin
    pub stdin: std::fs::File,
    /// PTY slave file for stdout
    pub stdout: std::fs::File,
    /// PTY slave file for stderr
    pub stderr: std::fs::File,
}

impl Pty {
    /// Creates a new PTY with the specified dimensions.
    ///
    /// # Arguments
    /// * `rows` - Number of rows for the PTY
    /// * `cols` - Number of columns for the PTY
    ///
    /// # Errors
    /// Returns an error if PTY creation fails or if file descriptor operations fail.
    pub fn new(rows: u16, cols: u16) -> Result<Self, std::io::Error> {
        // Create a PTY using libc directly so we can keep both master and slave fds
        let mut master_fd: libc::c_int = -1;
        let mut slave_fd: libc::c_int = -1;
        let winsize = libc::winsize {
            ws_row: rows,
            ws_col: cols,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };

        // SAFETY: openpty is called with valid pointers to uninitialized integers for fds,
        // null pointers for termios (using defaults), and a valid winsize struct.
        let result = unsafe {
            libc::openpty(
                std::ptr::addr_of_mut!(master_fd),
                std::ptr::addr_of_mut!(slave_fd),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::addr_of!(winsize).cast_mut(),
            )
        };

        if result != 0 {
            return Err(std::io::Error::last_os_error());
        }

        // Set close-on-exec for both fds
        // SAFETY: Set close-on-exec flag for both master and slave fds
        unsafe {
            let flags = libc::fcntl(master_fd, libc::F_GETFD);
            libc::fcntl(master_fd, libc::F_SETFD, flags | libc::FD_CLOEXEC);
            let flags = libc::fcntl(slave_fd, libc::F_GETFD);
            libc::fcntl(slave_fd, libc::F_SETFD, flags | libc::FD_CLOEXEC);
        }

        // Set up the VT100 parser with same dimensions as PTY
        // Use 10000 lines of scrollback to preserve command output
        let parser = Arc::new(RwLock::new(vt100::Parser::new(rows, cols, 10000)));

        // Spawn a thread to read from PTY master and update the parser
        {
            let parser = Arc::clone(&parser);
            // SAFETY: Duplicate master fd for reading
            let reader_fd = unsafe { libc::dup(master_fd) };
            if reader_fd < 0 {
                return Err(std::io::Error::last_os_error());
            }
            // SAFETY: We own reader_fd from the successful dup() call above
            let mut reader = unsafe { std::fs::File::from_raw_fd(reader_fd) };

            std::thread::spawn(move || {
                let mut buf = [0u8; 8192];
                loop {
                    match reader.read(&mut buf) {
                        Ok(0) => break,
                        Ok(size) => {
                            let mut parser = parser.write().unwrap();
                            parser.process(&buf[..size]);
                        }
                        Err(_) => break,
                    }
                }
            });
        }

        // Set up PTY writer channel
        let (tx, mut rx) = channel::<Bytes>(32);
        // SAFETY: Duplicate master fd for writing
        let writer_fd = unsafe { libc::dup(master_fd) };
        if writer_fd < 0 {
            return Err(std::io::Error::last_os_error());
        }
        // SAFETY: We own writer_fd from the successful dup() call above
        let writer = unsafe { std::fs::File::from_raw_fd(writer_fd) };
        let mut writer = BufWriter::new(writer);

        tokio::spawn(async move {
            while let Some(bytes) = rx.recv().await {
                let _ = writer.write_all(&bytes);
                let _ = writer.flush();
            }
        });

        // Create File handles from the slave fd
        // Duplicate the slave fd three times for stdin, stdout, stderr
        // SAFETY: Duplicate slave fd for stdin
        let slave_stdin = unsafe { std::fs::File::from_raw_fd(libc::dup(slave_fd)) };

        // SAFETY: Duplicate slave fd for stdout
        let slave_stdout = unsafe { std::fs::File::from_raw_fd(libc::dup(slave_fd)) };

        // SAFETY: Duplicate slave fd for stderr
        let slave_stderr = unsafe { std::fs::File::from_raw_fd(libc::dup(slave_fd)) };

        // SAFETY: Close the original slave fd since we've duplicated it three times
        unsafe {
            libc::close(slave_fd);
        }

        Ok(Self {
            parser,
            writer: tx,
            master_fd,
            stdin: slave_stdin,
            stdout: slave_stdout,
            stderr: slave_stderr,
        })
    }

    /// Returns a clone of the VT100 parser for reading terminal state.
    pub fn parser(&self) -> Arc<RwLock<vt100::Parser>> {
        Arc::clone(&self.parser)
    }

    /// Returns a clone of the writer channel for sending data to the PTY.
    pub fn writer(&self) -> Sender<Bytes> {
        self.writer.clone()
    }

    /// Resizes the PTY to the specified dimensions.
    ///
    /// Updates both the VT100 parser and sends TIOCSWINSZ to the PTY master.
    ///
    /// # Arguments
    /// * `rows` - New number of rows
    /// * `cols` - New number of columns
    ///
    /// # Errors
    /// Returns an error if the ioctl call fails.
    pub fn resize(&self, rows: u16, cols: u16) -> Result<(), std::io::Error> {
        // Update parser size
        let mut parser = self.parser.write().unwrap();
        parser.set_size(rows, cols);
        drop(parser); // Release lock

        // Update PTY window size via ioctl
        let winsize = libc::winsize {
            ws_row: rows,
            ws_col: cols,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };

        // SAFETY: ioctl with TIOCSWINSZ and valid winsize struct
        let result = unsafe {
            libc::ioctl(
                self.master_fd,
                libc::TIOCSWINSZ,
                std::ptr::addr_of!(winsize),
            )
        };

        if result != 0 {
            return Err(std::io::Error::last_os_error());
        }

        Ok(())
    }
}
