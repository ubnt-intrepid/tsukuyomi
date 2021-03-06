//! Definition of `Handler`.

use {
    crate::{
        error::Error,
        future::TryFuture,
        util::{Chain, Never, TryFrom}, //
    },
    http::{header::HeaderValue, HttpTryFrom, Method},
    indexmap::{indexset, IndexSet},
    lazy_static::lazy_static,
    std::{iter::FromIterator, sync::Arc},
};

/// A set of request methods that a route accepts.
#[derive(Debug, Clone)]
pub struct AllowedMethods(IndexSet<Method>);

impl AllowedMethods {
    pub fn get() -> &'static AllowedMethods {
        lazy_static! {
            static ref VALUE: AllowedMethods = AllowedMethods::from(Method::GET);
        }
        &*VALUE
    }

    pub fn contains(&self, method: &Method) -> bool {
        self.0.contains(method)
    }

    pub fn iter<'a>(&'a self) -> impl Iterator<Item = &'a Method> + 'a {
        self.0.iter()
    }

    pub fn to_header_value(&self) -> HeaderValue {
        let mut bytes = bytes::BytesMut::new();
        for (i, method) in self.iter().enumerate() {
            if i > 0 {
                bytes.extend_from_slice(b", ");
            }
            bytes.extend_from_slice(method.as_str().as_bytes());
        }
        unsafe { HeaderValue::from_shared_unchecked(bytes.freeze()) }
    }
}

impl From<Method> for AllowedMethods {
    fn from(method: Method) -> Self {
        AllowedMethods(indexset! { method })
    }
}

impl FromIterator<Method> for AllowedMethods {
    fn from_iter<T>(iter: T) -> Self
    where
        T: IntoIterator<Item = Method>,
    {
        AllowedMethods(FromIterator::from_iter(iter))
    }
}

impl Extend<Method> for AllowedMethods {
    fn extend<I: IntoIterator<Item = Method>>(&mut self, iterable: I) {
        self.0.extend(iterable)
    }
}

impl TryFrom<Self> for AllowedMethods {
    type Error = Never;

    #[inline]
    fn try_from(methods: Self) -> std::result::Result<Self, Self::Error> {
        Ok(methods)
    }
}

impl TryFrom<Method> for AllowedMethods {
    type Error = Never;

    #[inline]
    fn try_from(method: Method) -> std::result::Result<Self, Self::Error> {
        Ok(AllowedMethods::from(method))
    }
}

impl<M> TryFrom<Vec<M>> for AllowedMethods
where
    Method: HttpTryFrom<M>,
{
    type Error = http::Error;

    #[inline]
    fn try_from(methods: Vec<M>) -> std::result::Result<Self, Self::Error> {
        let methods: Vec<_> = methods
            .into_iter()
            .map(Method::try_from)
            .collect::<std::result::Result<_, _>>()
            .map_err(Into::into)?;
        Ok(AllowedMethods::from_iter(methods))
    }
}

impl<'a> TryFrom<&'a str> for AllowedMethods {
    type Error = failure::Error;

    #[inline]
    fn try_from(methods: &'a str) -> std::result::Result<Self, Self::Error> {
        let methods: Vec<_> = methods
            .split(',')
            .map(|s| Method::try_from(s.trim()).map_err(Into::into))
            .collect::<http::Result<_>>()?;
        Ok(AllowedMethods::from_iter(methods))
    }
}

impl IntoIterator for AllowedMethods {
    type Item = Method;
    type IntoIter = indexmap::set::IntoIter<Method>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a> IntoIterator for &'a AllowedMethods {
    type Item = &'a Method;
    type IntoIter = indexmap::set::Iter<'a, Method>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

/// A trait representing the handler associated with the specified endpoint.
pub trait Handler {
    type Output;
    type Error: Into<Error>;
    type Handle: TryFuture<Ok = Self::Output, Error = Self::Error>;

    /// Returns a list of HTTP methods that this handler accepts.
    ///
    /// If it returns a `None`, it means that the handler accepts *all* methods.
    fn allowed_methods(&self) -> Option<&AllowedMethods>;

    /// Creates a `Handle` which handles the incoming request.
    fn handle(&self) -> Self::Handle;
}

impl<H> Handler for std::rc::Rc<H>
where
    H: Handler,
{
    type Output = H::Output;
    type Error = H::Error;
    type Handle = H::Handle;

    #[inline]
    fn allowed_methods(&self) -> Option<&AllowedMethods> {
        (**self).allowed_methods()
    }

    #[inline]
    fn handle(&self) -> Self::Handle {
        (**self).handle()
    }
}

impl<H> Handler for Arc<H>
where
    H: Handler,
{
    type Output = H::Output;
    type Error = H::Error;
    type Handle = H::Handle;

    #[inline]
    fn allowed_methods(&self) -> Option<&AllowedMethods> {
        (**self).allowed_methods()
    }

    #[inline]
    fn handle(&self) -> Self::Handle {
        (**self).handle()
    }
}

pub fn handler<T>(
    handle_fn: impl Fn() -> T,
    allowed_methods: Option<AllowedMethods>,
) -> impl Handler<
    Output = T::Ok, //
    Error = T::Error,
    Handle = T,
>
where
    T: TryFuture,
{
    #[allow(missing_debug_implementations)]
    struct HandlerFn<F> {
        handle_fn: F,
        allowed_methods: Option<AllowedMethods>,
    }

    impl<F, T> Handler for HandlerFn<F>
    where
        F: Fn() -> T,
        T: TryFuture,
    {
        type Output = T::Ok;
        type Error = T::Error;
        type Handle = T;

        #[inline]
        fn allowed_methods(&self) -> Option<&AllowedMethods> {
            self.allowed_methods.as_ref()
        }

        #[inline]
        fn handle(&self) -> Self::Handle {
            (self.handle_fn)()
        }
    }

    HandlerFn {
        handle_fn,
        allowed_methods,
    }
}

/// A trait representing a type for modifying the instance of `Handler`.
pub trait ModifyHandler<H: Handler> {
    type Output;
    type Handler: Handler<Output = Self::Output>;

    fn modify(&self, input: H) -> Self::Handler;
}

#[doc(hidden)]
#[deprecated(
    since = "0.5.2",
    note = "this function will be removed in the next version."
)]
pub fn modify_handler<In, Out>(
    modify: impl Fn(In) -> Out,
) -> impl ModifyHandler<
    In, //
    Output = Out::Output,
    Handler = Out,
>
where
    In: Handler,
    Out: Handler,
{
    #[allow(missing_debug_implementations)]
    struct ModifyHandlerFn<F>(F);

    impl<F, In, Out> ModifyHandler<In> for ModifyHandlerFn<F>
    where
        F: Fn(In) -> Out,
        In: Handler,
        Out: Handler,
    {
        type Output = Out::Output;
        type Handler = Out;

        #[inline]
        fn modify(&self, inner: In) -> Self::Handler {
            (self.0)(inner)
        }
    }

    ModifyHandlerFn(modify)
}

impl<'a, M, H> ModifyHandler<H> for &'a M
where
    M: ModifyHandler<H>,
    H: Handler,
{
    type Output = M::Output;
    type Handler = M::Handler;

    #[inline]
    fn modify(&self, input: H) -> Self::Handler {
        (**self).modify(input)
    }
}

impl<M, H> ModifyHandler<H> for std::rc::Rc<M>
where
    M: ModifyHandler<H>,
    H: Handler,
{
    type Output = M::Output;
    type Handler = M::Handler;

    #[inline]
    fn modify(&self, input: H) -> Self::Handler {
        (**self).modify(input)
    }
}

impl<M, H> ModifyHandler<H> for std::sync::Arc<M>
where
    M: ModifyHandler<H>,
    H: Handler,
{
    type Output = M::Output;
    type Handler = M::Handler;

    #[inline]
    fn modify(&self, input: H) -> Self::Handler {
        (**self).modify(input)
    }
}

impl<H> ModifyHandler<H> for ()
where
    H: Handler,
{
    type Output = H::Output;
    type Handler = H;

    #[inline]
    fn modify(&self, input: H) -> Self::Handler {
        input
    }
}

impl<I, O, H> ModifyHandler<H> for Chain<I, O>
where
    H: Handler,
    I: ModifyHandler<H>,
    O: ModifyHandler<I::Handler>,
{
    type Output = O::Output;
    type Handler = O::Handler;

    #[inline]
    fn modify(&self, input: H) -> Self::Handler {
        self.right.modify(self.left.modify(input))
    }
}
