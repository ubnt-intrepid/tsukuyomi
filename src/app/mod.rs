//! The definition of components for constructing the HTTP applications.

pub mod service;

use failure::Error;
use state::Container;
use std::sync::Arc;
use std::{fmt, mem};

#[cfg(feature = "session")]
use session::{self, SessionStorage};

use error::handler::{DefaultErrorHandler, ErrorHandler};
use modifier::Modifier;
use router::{self, Mount, Router};

scoped_thread_local!(static STATE: AppState);

/// The global and shared variables used throughout the serving an HTTP application.
pub struct AppState {
    router: Router,
    error_handler: Box<ErrorHandler + Send + Sync + 'static>,
    modifiers: Vec<Box<Modifier + Send + Sync + 'static>>,
    states: Container,
    #[cfg(feature = "session")]
    session: SessionStorage,
}

impl fmt::Debug for AppState {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("AppState").field("router", &self.router).finish()
    }
}

impl AppState {
    pub(crate) fn set<R>(&self, f: impl FnOnce() -> R) -> R {
        STATE.set(self, f)
    }

    /// Returns the reference to `Router` contained in this value.
    pub fn router(&self) -> &Router {
        &self.router
    }

    /// Returns the reference to `ErrorHandler` contained in this value.
    pub fn error_handler(&self) -> &ErrorHandler {
        &*self.error_handler
    }

    /// Returns the reference to `ErrorHandler` contained in this value.
    pub fn modifiers(&self) -> &[Box<Modifier + Send + Sync + 'static>] {
        &self.modifiers
    }

    /// Returns the reference to a value of `T` from the global storage.
    ///
    /// If the value is not registered, it returns a `None`.
    pub fn state<T>(&self) -> Option<&T>
    where
        T: Send + Sync + 'static,
    {
        self.states.try_get()
    }

    /// Returns the reference to session manager.
    ///
    /// This method is available only if the feature `session` is enabled.
    #[cfg(feature = "session")]
    pub fn session(&self) -> &SessionStorage {
        &self.session
    }
}

/// The main type in this framework, which represents an HTTP application.
#[derive(Debug)]
pub struct App {
    state: Arc<AppState>,
}

impl App {
    /// Creates a builder object for constructing an instance of this type.
    pub fn builder() -> AppBuilder {
        AppBuilder {
            router: Router::builder(),
            error_handler: None,
            modifiers: vec![],
            states: Container::new(),
            #[cfg(feature = "session")]
            session: SessionStorage::builder(),
        }
    }
}

/// A builder object for constructing an instance of `App`.
pub struct AppBuilder {
    router: router::Builder,
    error_handler: Option<Box<ErrorHandler + Send + Sync + 'static>>,
    modifiers: Vec<Box<Modifier + Send + Sync + 'static>>,
    states: Container,
    #[cfg(feature = "session")]
    session: session::Builder,
}

impl fmt::Debug for AppBuilder {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("AppBuilder").field("router", &self.router).finish()
    }
}

impl AppBuilder {
    /// Registers some handlers to the router, with mounting on the specified prefix.
    ///
    /// See the documentation of `Mount` for details.
    ///
    /// # Examples
    ///
    /// ```
    /// # use tsukuyomi::{App, Input};
    /// # use tsukuyomi::future::ready;
    /// # let index = |_: &Input| ready("a");
    /// # let find_post = |_: &Input| ready("a");
    /// # let all_posts = |_: &Input| ready("a");
    /// # let add_post = |_: &Input| ready("a");
    /// let app = App::builder()
    ///     .mount("/", |r| { r.get("/", index); })
    ///     .mount("/api/v1/", |r| {
    ///         r.get("/posts/:id", find_post);
    ///         r.get("/posts", all_posts);
    ///         r.post("/posts", add_post);
    ///     })
    ///     .finish();
    /// ```
    pub fn mount(&mut self, base: &str, f: impl FnOnce(&mut Mount)) -> &mut Self {
        f(&mut self.router.mount(base));
        self
    }

    /// Modifies the router level configurations.
    pub fn router(&mut self, f: impl FnOnce(&mut router::Builder)) -> &mut Self {
        f(&mut self.router);
        self
    }

    /// Sets the instance to an error handler into this builder.
    pub fn error_handler<H>(&mut self, error_handler: H) -> &mut Self
    where
        H: ErrorHandler + Send + Sync + 'static,
    {
        self.error_handler = Some(Box::new(error_handler));
        self
    }

    /// Sets the instance to an error handler into this builder.
    pub fn modifier<M>(&mut self, modifier: M) -> &mut Self
    where
        M: Modifier + Send + Sync + 'static,
    {
        self.modifiers.push(Box::new(modifier));
        self
    }

    /// Sets a value of `T` to the global storage.
    ///
    /// If a value of provided type has already set, this method drops `state` immediately
    /// and does not provide any affects to the global storage.
    pub fn manage<T>(&mut self, state: T) -> &mut Self
    where
        T: Send + Sync + 'static,
    {
        self.states.set(state);
        self
    }

    /// Modifies the configuration of session manager.
    ///
    /// This method is available only if the feature `session` is enabled.
    #[cfg(feature = "session")]
    pub fn session(&mut self, f: impl FnOnce(&mut session::Builder)) -> &mut Self {
        f(&mut self.session);
        self
    }

    /// Creates a configured `App` from the current configuration.
    pub fn finish(&mut self) -> Result<App, Error> {
        let mut builder = mem::replace(self, App::builder());
        builder.states.freeze();

        let state = AppState {
            router: builder.router.finish()?,
            error_handler: builder
                .error_handler
                .unwrap_or_else(|| Box::new(DefaultErrorHandler::new())),
            modifiers: builder.modifiers,
            states: builder.states,
            #[cfg(feature = "session")]
            session: builder.session.finish(),
        };

        Ok(App { state: Arc::new(state) })
    }
}
