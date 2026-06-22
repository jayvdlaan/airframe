//! Unix domain socket channel with length-prefixed framing.

use spacetime_ipc::{IpcChannel, IpcError};
use std::io::{Read, Write};
use std::os::unix::io::AsRawFd;
use std::os::unix::net::UnixStream;
use std::path::Path;

/// Maximum message size (4 MB -- sufficient for control messages).
pub const MAX_MESSAGE_SIZE: usize = 4 * 1024 * 1024;

/// A synchronous Unix domain socket channel with 4-byte big-endian
/// length-prefixed message framing.
pub struct UnixSocketChannel {
    stream: UnixStream,
    read_buf: Vec<u8>,
}

impl UnixSocketChannel {
    /// Connect to a UDS at the given path.
    pub fn connect<P: AsRef<Path>>(path: P) -> Result<Self, IpcError> {
        let stream = UnixStream::connect(path).map_err(|_| IpcError::SendFailed)?;
        Ok(Self {
            stream,
            read_buf: Vec::new(),
        })
    }

    /// Create a channel from an already-connected `UnixStream`.
    pub fn from_stream(stream: UnixStream) -> Self {
        Self {
            stream,
            read_buf: Vec::new(),
        }
    }

    /// Set non-blocking mode.
    pub fn set_nonblocking(&self, nonblocking: bool) -> Result<(), IpcError> {
        self.stream
            .set_nonblocking(nonblocking)
            .map_err(|_| IpcError::SendFailed)
    }
}

impl IpcChannel for UnixSocketChannel {
    fn send(&mut self, data: &[u8]) -> Result<(), IpcError> {
        if data.len() > MAX_MESSAGE_SIZE {
            return Err(IpcError::InvalidArgument);
        }
        let len = data.len() as u32;
        self.stream
            .write_all(&len.to_be_bytes())
            .map_err(|_| IpcError::SendFailed)?;
        self.stream
            .write_all(data)
            .map_err(|_| IpcError::SendFailed)?;
        self.stream.flush().map_err(|_| IpcError::SendFailed)?;
        Ok(())
    }

    fn recv(&mut self, buf: &mut [u8]) -> Result<usize, IpcError> {
        let mut header = [0u8; 4];
        self.stream.read_exact(&mut header).map_err(|e| {
            if e.kind() == std::io::ErrorKind::UnexpectedEof {
                IpcError::ConnectionClosed
            } else {
                IpcError::RecvFailed
            }
        })?;

        let len = u32::from_be_bytes(header) as usize;
        if len > MAX_MESSAGE_SIZE {
            return Err(IpcError::InvalidArgument);
        }

        // Read into internal buffer if provided buf is too small
        if len > buf.len() {
            self.read_buf.resize(len, 0);
            self.stream
                .read_exact(&mut self.read_buf[..len])
                .map_err(|e| {
                    if e.kind() == std::io::ErrorKind::UnexpectedEof {
                        IpcError::ConnectionClosed
                    } else {
                        IpcError::RecvFailed
                    }
                })?;
            buf.copy_from_slice(&self.read_buf[..buf.len()]);
            Ok(len)
        } else {
            self.stream.read_exact(&mut buf[..len]).map_err(|e| {
                if e.kind() == std::io::ErrorKind::UnexpectedEof {
                    IpcError::ConnectionClosed
                } else {
                    IpcError::RecvFailed
                }
            })?;
            Ok(len)
        }
    }

    fn poll(&self) -> bool {
        // Use recv with MSG_PEEK | MSG_DONTWAIT to check for data
        // without consuming it and without blocking.
        let mut peek_buf = [0u8; 1];
        let ret = unsafe {
            libc::recv(
                self.stream.as_raw_fd(),
                peek_buf.as_mut_ptr() as *mut libc::c_void,
                1,
                libc::MSG_PEEK | libc::MSG_DONTWAIT,
            )
        };
        ret > 0
    }
}

/// Receive a message, allocating a new `Vec`.
pub fn recv_vec(channel: &mut UnixSocketChannel) -> Result<Vec<u8>, IpcError> {
    let mut buf = vec![0u8; MAX_MESSAGE_SIZE];
    let n = channel.recv(&mut buf)?;
    buf.truncate(n);
    Ok(buf)
}
