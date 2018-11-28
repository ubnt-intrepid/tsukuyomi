use {
    super::{
        builder::AppContext,
        error::{Error, Result},
        fallback::{Fallback, FallbackInstance},
    },
    crate::{common::Never, modifier::Modifier, scoped_map::ScopeId, uri::Uri},
};

#[allow(deprecated)]
use super::route::Route;

pub trait Scope {
    type Error: Into<Error>;

    fn configure(self, cx: &mut Context<'_>) -> std::result::Result<(), Self::Error>;
}

impl Scope for () {
    type Error = Never;

    fn configure(self, _: &mut Context<'_>) -> std::result::Result<(), Self::Error> {
        Ok(())
    }
}

pub(super) fn raw<F, E>(f: F) -> impl Scope<Error = E>
where
    F: FnOnce(&mut Context<'_>) -> std::result::Result<(), E>,
    E: Into<Error>,
{
    #[allow(missing_debug_implementations)]
    struct Raw<F>(F);

    impl<F, E> Scope for Raw<F>
    where
        F: FnOnce(&mut Context<'_>) -> std::result::Result<(), E>,
        E: Into<Error>,
    {
        type Error = E;

        fn configure(self, cx: &mut Context<'_>) -> std::result::Result<(), Self::Error> {
            (self.0)(cx)
        }
    }

    Raw(f)
}

#[derive(Debug, Default)]
pub struct Builder<S: Scope = ()> {
    pub(super) scope: S,
}

#[cfg_attr(feature = "cargo-clippy", allow(use_self))]
impl<S> Builder<S>
where
    S: Scope,
{
    /// Adds a route into this scope.
    #[deprecated(since = "0.4.1", note = "use Builder::with(route) instead.")]
    #[allow(deprecated)]
    pub fn route(self, route: impl Route) -> Builder<impl Scope<Error = Error>> {
        Builder {
            scope: raw(move |cx| {
                self.scope.configure(cx).map_err(Into::into)?;
                cx.add_route(route)?;
                Ok(())
            }),
        }
    }

    /// Create a new subscope onto this scope.
    #[inline]
    pub fn mount(self, new_scope: impl Scope) -> Builder<impl Scope<Error = Error>> {
        Builder {
            scope: raw(move |cx| {
                self.scope.configure(cx).map_err(Into::into)?;
                cx.add_scope(new_scope)?;
                Ok(())
            }),
        }
    }

    /// Merges the specified `Scope` into this scope, *without* creating a new subscope.
    pub fn with(self, next_scope: impl Scope) -> Builder<impl Scope<Error = Error>> {
        Builder {
            scope: raw(move |cx| {
                self.scope.configure(cx).map_err(Into::into)?;
                next_scope.configure(cx).map_err(Into::into)?;
                Ok(())
            }),
        }
    }

    /// Registers a shared variable into this scope.
    pub fn state<T>(self, state: T) -> Builder<impl Scope<Error = S::Error>>
    where
        T: Send + Sync + 'static,
    {
        Builder {
            scope: raw(move |cx| {
                self.scope.configure(cx)?;
                cx.set_state(state);
                Ok(())
            }),
        }
    }

    /// Registers a `Modifier` into this scope.
    pub fn modifier(
        self,
        modifier: impl Modifier + Send + Sync + 'static,
    ) -> Builder<impl Scope<Error = S::Error>> {
        Builder {
            scope: raw(move |cx| {
                self.scope.configure(cx)?;
                cx.add_modifier(modifier);
                Ok(())
            }),
        }
    }

    /// Registers a `Fallback` into this scope.
    pub fn fallback(
        self,
        fallback: impl Fallback + Send + Sync + 'static,
    ) -> Builder<impl Scope<Error = S::Error>> {
        self.state(FallbackInstance::from(fallback))
    }

    /// Set the prefix URL of this scope.
    pub fn prefix(self, prefix: Uri) -> Builder<impl Scope<Error = Error>> {
        Builder {
            scope: raw(move |cx| {
                cx.set_prefix(prefix)?;
                self.scope.configure(cx).map_err(Into::into)?;
                Ok(())
            }),
        }
    }
}

impl<S> Scope for Builder<S>
where
    S: Scope,
{
    type Error = S::Error;

    #[inline]
    fn configure(self, cx: &mut Context<'_>) -> std::result::Result<(), Self::Error> {
        self.scope.configure(cx)
    }
}

/// A proxy object for configuration of a scope.
#[derive(Debug)]
pub struct Context<'a> {
    cx: &'a mut AppContext,
    id: ScopeId,
}

impl<'a> Context<'a> {
    pub(super) fn new(cx: &'a mut AppContext, id: ScopeId) -> Self {
        Self { cx, id }
    }

    /// Adds a route into the current scope.
    // note:
    // Currently, this method is only called in `fs::Staticfiles`
    // to add routes. In order to provide the implementors of `Scope`
    // that adds some route(s) dynamically, the context need to provide
    // the similar API.
    #[deprecated(
        since = "0.4.1",
        note = "This method will be removed in the next version."
    )]
    #[allow(deprecated)]
    pub fn add_route<R>(&mut self, route: R) -> Result<()>
    where
        R: Route,
    {
        self.cx.new_route(self.id, route)
    }

    /// Create a new scope mounted to the certain URI.
    #[inline]
    fn add_scope<S>(&mut self, new_scope: S) -> Result<()>
    where
        S: Scope,
    {
        self.cx.new_scope(self.id, new_scope)
    }

    /// Adds a *scope-local* variable into the application.
    pub fn set_state<T>(&mut self, value: T)
    where
        T: Send + Sync + 'static,
    {
        self.cx.set_state(value, self.id)
    }

    pub fn add_modifier<M>(&mut self, modifier: M)
    where
        M: Modifier + Send + Sync + 'static,
    {
        self.cx.add_modifier(modifier, self.id)
    }

    pub fn set_prefix(&mut self, prefix: Uri) -> super::Result<()> {
        self.cx.set_prefix(self.id, prefix)
    }
}
