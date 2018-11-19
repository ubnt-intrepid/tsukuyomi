//! The definition of `Modifier`.
//!
//! The purpose of this trait is to insert some processes before and after
//! applying `Handler` in a certain scope.
//!
//! # Examples
//!
//! ```
//! # extern crate tsukuyomi;
//! use std::sync::atomic::{AtomicUsize, Ordering};
//! use tsukuyomi::{
//!     app::route,
//!     output::Output,
//!     handler::AsyncResult,
//!     Modifier,
//! };
//!
//! #[derive(Default)]
//! struct RequestCounter(AtomicUsize);
//!
//! impl Modifier for RequestCounter {
//!     fn modify(&self, result: AsyncResult<Output>) -> AsyncResult<Output> {
//!        self.0.fetch_add(1, Ordering::SeqCst);
//!        result
//!     }
//! }
//!
//! # fn main() -> tsukuyomi::app::Result<()> {
//! tsukuyomi::app!()
//!     .route(route!().reply(|| "Hello"))
//!     .modifier(RequestCounter::default())
//!     .build()
//! #   .map(drop)
//! # }
//! ```

use crate::{handler::AsyncResult, output::Output};

/// A trait representing a `Modifier`.
pub trait Modifier {
    #[allow(unused_variables)]
    fn setup(&mut self, cx: &mut crate::app::scope::Context<'_>) -> crate::app::Result<()> {
        Ok(())
    }

    fn modify(&self, result: AsyncResult<Output>) -> AsyncResult<Output>;
}

impl Modifier for () {
    #[inline]
    fn modify(&self, result: AsyncResult<Output>) -> AsyncResult<Output> {
        result
    }
}

#[derive(Debug)]
pub struct Chain<M1, M2> {
    m1: M1,
    m2: M2,
}

impl<M1, M2> Chain<M1, M2> {
    pub(super) fn new(m1: M1, m2: M2) -> Self {
        Self { m1, m2 }
    }
}

impl<M1, M2> Modifier for Chain<M1, M2>
where
    M1: Modifier,
    M2: Modifier,
{
    fn setup(&mut self, cx: &mut crate::app::scope::Context<'_>) -> crate::app::Result<()> {
        self.m1.setup(cx)?;
        self.m2.setup(cx)?;
        Ok(())
    }

    fn modify(&self, result: AsyncResult<Output>) -> AsyncResult<Output> {
        self.m1.modify(self.m2.modify(result))
    }
}
