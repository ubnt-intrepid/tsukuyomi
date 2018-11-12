//! Components for accessing HTTP requests and global/request-local data.

pub mod body;
mod global;
mod param;

// re-exports
pub use self::body::RequestBody;
pub use self::global::{is_set_current, with_get_current};
pub use self::param::Params;
pub use crate::internal::local_map;

pub(crate) use self::global::with_set_current;

// ====

use std::marker::PhantomData;
use std::rc::Rc;

use cookie::{Cookie, CookieJar};
use futures::{Future, IntoFuture};
use http::Request;
use mime::Mime;

use crate::app::service::AppContext;
use crate::app::App;
use crate::error::Error;

use self::local_map::LocalMap;

/// Contextual information used by processes during an incoming HTTP request.
#[derive(Debug)]
pub struct Input<'task> {
    request: &'task Request<()>,
    app: &'task App,
    context: &'task mut AppContext,
}

impl<'task> Input<'task> {
    pub(super) fn new(
        request: &'task Request<()>,
        app: &'task App,
        context: &'task mut AppContext,
    ) -> Self {
        Self {
            request,
            app,
            context,
        }
    }

    /// Returns a reference to the HTTP method of the request.
    #[inline]
    #[cfg_attr(tarpaulin, skip)]
    pub fn method(&self) -> &http::Method {
        self.request.method()
    }

    /// Returns a reference to the URI of the request.
    #[inline]
    #[cfg_attr(tarpaulin, skip)]
    pub fn uri(&self) -> &http::Uri {
        self.request.uri()
    }

    /// Returns a reference to the HTTP version of the request.
    #[inline]
    #[cfg_attr(tarpaulin, skip)]
    pub fn version(&self) -> http::Version {
        self.request.version()
    }

    /// Returns a reference to the header map in the request.
    #[inline]
    #[cfg_attr(tarpaulin, skip)]
    pub fn headers(&self) -> &http::HeaderMap {
        self.request.headers()
    }

    /// Returns a reference to the extensions map in the request.
    #[inline]
    #[cfg_attr(tarpaulin, skip)]
    pub fn extensions(&self) -> &http::Extensions {
        self.request.extensions()
    }

    /// Creates an instance of "Payload" from the raw message body.
    pub fn take_body(&mut self) -> Option<self::body::RequestBody> {
        self.context.body.take()
    }

    /// Creates an instance of "ReadAll" from the raw message body.
    pub fn read_all(&mut self) -> self::body::ReadAll {
        self::body::ReadAll::init(self.take_body())
    }

    /// Returns 'true' if the upgrade function is set.
    pub fn is_upgraded(&self) -> bool {
        self.context.is_upgraded
    }

    /// Registers the upgrade function to this request.
    pub fn upgrade<F, R>(&mut self, on_upgrade: F) -> Result<(), F>
    where
        F: FnOnce(self::body::UpgradedIo) -> R + Send + 'static,
        R: IntoFuture<Item = (), Error = ()>,
        R::Future: Send + 'static,
    {
        if self.is_upgraded() {
            return Err(on_upgrade);
        }
        self.context.is_upgraded = true;

        let body = self.take_body().expect("The body has already gone");
        crate::rt::spawn(
            body.on_upgrade()
                .map_err(|_| ())
                .and_then(move |upgraded| on_upgrade(upgraded).into_future()),
        );

        Ok(())
    }

    /// Returns a reference to the parsed value of `Content-type` stored in the specified `Input`.
    pub fn content_type(&mut self) -> Result<Option<&Mime>, Error> {
        use self::local_map::{local_key, Entry};

        local_key!(static KEY: Option<Mime>);

        match self.context.locals.entry(&KEY) {
            Entry::Occupied(entry) => Ok(entry.into_mut().as_ref()),
            Entry::Vacant(entry) => {
                let mime = match self.request.headers().get(http::header::CONTENT_TYPE) {
                    Some(h) => h
                        .to_str()
                        .map_err(crate::error::bad_request)?
                        .parse()
                        .map(Some)
                        .map_err(crate::error::bad_request)?,
                    None => None,
                };
                Ok(entry.insert(mime).as_ref())
            }
        }
    }

    /// Returns a proxy object for accessing parameters extracted by the router.
    pub fn params(&self) -> self::param::Params<'_> {
        self::param::Params::new(
            self.request.uri().path(),
            self.app.uri(self.context.route_id()).capture_names(),
            self.context.captures.as_ref(),
        )
    }

    /// Returns the reference to a value of `T` registered in the global storage, if possible.
    ///
    /// This method will return a `None` if a value of `T` is not registered in the global storage.
    #[inline]
    pub fn state<T>(&self) -> Option<&T>
    where
        T: Send + Sync + 'static,
    {
        self.app.get_state(self.context.route_id())
    }

    /// Returns a proxy object for managing the value of Cookie entries.
    ///
    /// This function will perform parsing when called at first, and returns an `Err`
    /// if the value of header field is invalid.
    pub fn cookies(&mut self) -> Result<Cookies<'_>, Error> {
        self.context
            .init_cookie_jar(self.request.headers())
            .map(|jar| Cookies {
                jar,
                _marker: PhantomData,
            })
    }

    /// Returns a reference to `LocalMap` for managing request-local data.
    #[cfg_attr(tarpaulin, skip)]
    pub fn locals(&self) -> &LocalMap {
        &self.context.locals
    }

    /// Returns a mutable reference to `LocalMap` for managing request-local data.
    #[cfg_attr(tarpaulin, skip)]
    pub fn locals_mut(&mut self) -> &mut LocalMap {
        &mut self.context.locals
    }
}

/// A proxy object for accessing Cookie values.
///
/// Currently this type is a thin wrapper of `&mut cookie::CookieJar`.
#[derive(Debug)]
pub struct Cookies<'a> {
    jar: &'a mut CookieJar,
    _marker: PhantomData<Rc<()>>,
}

impl<'a> Cookies<'a> {
    /// Returns a reference to a Cookie value with the specified name.
    #[inline]
    pub fn get(&self, name: &str) -> Option<&Cookie<'static>> {
        self.jar.get(name)
    }

    /// Adds a Cookie entry into jar.
    #[inline]
    pub fn add(&mut self, cookie: Cookie<'static>) {
        self.jar.add(cookie);
    }

    /// Removes a Cookie entry from jar.
    #[inline]
    pub fn remove(&mut self, cookie: Cookie<'static>) {
        self.jar.remove(cookie);
    }

    /// Removes a Cookie entry *completely*.
    #[inline]
    pub fn force_remove(&mut self, cookie: Cookie<'static>) {
        self.jar.force_remove(cookie);
    }
}

#[cfg(feature = "secure")]
mod secure {
    use cookie::{Key, PrivateJar, SignedJar};

    impl<'a> super::Cookies<'a> {
        /// Creates a `SignedJar` with the specified secret key.
        #[inline]
        pub fn signed(&mut self, key: &Key) -> SignedJar<'_> {
            self.jar.signed(key)
        }

        /// Creates a `PrivateJar` with the specified secret key.
        #[inline]
        pub fn private(&mut self, key: &Key) -> PrivateJar<'_> {
            self.jar.private(key)
        }
    }
}
