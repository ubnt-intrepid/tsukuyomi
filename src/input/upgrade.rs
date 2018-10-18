//! Components for the basic mechanisim for HTTP/1.1 server upgrade.
//!
//! # Examples
//!
//! ```
//! # extern crate tsukuyomi;
//! # extern crate futures;
//! # extern crate http;
//! # use tsukuyomi::error::Error;
//! # use tsukuyomi::input::Input;
//! # use tsukuyomi::input::upgrade::UpgradedIo;
//! # use tsukuyomi::output::{Output, ResponseBody};
//! # use futures::{future, Future};
//! # use http::{header, StatusCode, Response};
//! # #[allow(unused_variables, dead_code)]
//! fn validate(input: &Input) -> Result<(), Error> {
//!     // do some stuff ...
//! #   Ok(())
//! }
//!
//! # #[allow(unused_variables, dead_code)]
//! fn on_upgrade(io: UpgradedIo)
//!     -> impl Future<Item = (), Error = ()> + Send + 'static {
//!     // ...
//! #   future::ok(())
//! }
//!
//! # #[allow(dead_code)]
//! fn handshake(input: &mut Input) -> Result<Output, Error> {
//!     validate(input)?;
//!
//!     // Register a callback function called when upgrading
//!     // the server protocol.
//!     let _ = input.body_mut().upgrade(on_upgrade);
//!
//!     // Build the handshake response.
//!     // If the status code is set to `101 Switching Protocols`,
//!     // a task will be generated by calling a callback function
//!     // registered at the above section at the end of handling
//!     // the request.
//!     Ok(Response::builder()
//!         .status(StatusCode::SWITCHING_PROTOCOLS)
//!         .header(header::UPGRADE, "foo")
//!         .body(ResponseBody::empty())
//!         .unwrap())
//! }
//! ```

use bytes::{Buf, BufMut};
use futures::Poll;
use hyper::upgrade::Upgraded;
use std::io;
use tokio_io;

/// An asynchronous I/O upgraded from HTTP connection.
///
/// Currenly, this type is implemented as a thin wrapper of `hyper::upgrade::Upgraded`.
#[derive(Debug)]
pub struct UpgradedIo(pub(crate) Upgraded);

impl io::Read for UpgradedIo {
    #[inline]
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.read(buf)
    }
}

impl io::Write for UpgradedIo {
    #[inline]
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.write(buf)
    }

    #[inline]
    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

impl tokio_io::AsyncRead for UpgradedIo {
    #[inline]
    unsafe fn prepare_uninitialized_buffer(&self, buf: &mut [u8]) -> bool {
        self.0.prepare_uninitialized_buffer(buf)
    }

    #[inline]
    fn read_buf<B: BufMut>(&mut self, buf: &mut B) -> Poll<usize, io::Error> {
        self.0.read_buf(buf)
    }
}

impl tokio_io::AsyncWrite for UpgradedIo {
    #[inline]
    fn shutdown(&mut self) -> Poll<(), io::Error> {
        tokio_io::AsyncWrite::shutdown(&mut self.0)
    }

    #[inline]
    fn write_buf<B: Buf>(&mut self, buf: &mut B) -> Poll<usize, io::Error> {
        self.0.write_buf(buf)
    }
}
