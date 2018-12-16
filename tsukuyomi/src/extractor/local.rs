//! Extractors for accessing the request-local data.

use {
    super::Extractor,
    crate::{error::Error, input::localmap::LocalKey},
    futures01::Future,
};

pub fn remove<T>(
    key: &'static LocalKey<T>,
) -> impl Extractor<
    Output = (T,), //
    Error = Error,
    Future = impl Future<Item = (T,), Error = Error> + Send + 'static,
>
where
    T: Send + 'static,
{
    super::ready(move |input| input.locals.remove(key).ok_or_else(missing_local_value))
}

pub fn clone<T>(
    key: &'static LocalKey<T>,
) -> impl Extractor<
    Output = (T,), //
    Error = Error,
    Future = impl Future<Item = (T,), Error = Error> + Send + 'static,
>
where
    T: Clone + Send + 'static,
{
    super::ready(move |input| {
        input
            .locals
            .get(key)
            .cloned()
            .ok_or_else(missing_local_value)
    })
}

fn missing_local_value() -> crate::Error {
    crate::error::internal_server_error("missing local value")
}
