use {
    super::{
        fallback::{Context as FallbackContext, FallbackKind},
        recognizer::Captures,
        AppInner, ResourceId, Route,
    },
    crate::{
        error::Critical,
        handler::{Handle, HandleFn, HandleInner},
        input::{body::RequestBody, localmap::LocalMap, param::Params, Cookies, Input},
        output::{Output, ResponseBody},
    },
    cookie::CookieJar,
    futures01::{Async, Future, Poll},
    http::{
        header::{self, HeaderValue},
        Request, Response,
    },
    hyper::body::Payload,
    std::{fmt, marker::PhantomData, sync::Arc},
};

macro_rules! ready {
    ($e:expr) => {
        match $e {
            Ok(Async::NotReady) => return Ok(Async::NotReady),
            Ok(Async::Ready(x)) => Ok(x),
            Err(e) => Err(e),
        }
    };
}

/// A future that manages an HTTP request, created by `AppService`.
#[must_use = "futures do nothing unless polled"]
#[derive(Debug)]
pub struct AppFuture {
    request: Request<()>,
    inner: Arc<AppInner>,
    cookie_jar: Option<CookieJar>,
    locals: LocalMap,
    resource_id: Option<ResourceId>,
    captures: Option<Captures>,
    state: AppFutureState,
}

enum AppFutureState {
    Init,
    InFlight(Box<HandleFn>),
    Done,
}

impl fmt::Debug for AppFutureState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppFutureState::Init => f.debug_struct("Init").finish(),
            AppFutureState::InFlight(..) => f.debug_struct("InFlight").finish(),
            AppFutureState::Done => f.debug_struct("Done").finish(),
        }
    }
}

macro_rules! input {
    ($self:expr) => {
        &mut Input {
            request: &$self.request,
            params: {
                &if let Some(resource_id) = $self.resource_id {
                    Some(Params {
                        path: $self.request.uri().path(),
                        names: $self.inner.resource(resource_id).uri.capture_names(),
                        captures: $self.captures.as_ref(),
                    })
                } else {
                    None
                }
            },
            cookies: &mut Cookies::new(&mut $self.cookie_jar, &$self.request),
            locals: &mut $self.locals,
            _marker: PhantomData,
        }
    };
}

impl AppFuture {
    pub(super) fn new(request: Request<RequestBody>, inner: Arc<AppInner>) -> Self {
        let (parts, body) = request.into_parts();
        let mut locals = LocalMap::default();
        locals.insert(&RequestBody::KEY, body);
        Self {
            request: Request::from_parts(parts, ()),
            inner,
            cookie_jar: None,
            locals,
            resource_id: None,
            captures: None,
            state: AppFutureState::Init,
        }
    }

    fn process_recognize(&mut self) -> Handle {
        match self
            .inner
            .route(self.request.uri().path(), self.request.method())
        {
            Route::FoundEndpoint {
                endpoint,
                resource,
                captures,
                ..
            } => {
                self.resource_id = Some(resource.id);
                self.captures = captures;
                endpoint.handler.call(input!(self))
            }

            Route::FoundResource {
                resource, captures, ..
            } => {
                self.resource_id = Some(resource.id);
                self.captures = captures;
                let kind = FallbackKind::FoundResource(resource);
                match resource.fallback {
                    Some(ref fallback) => fallback.call(&mut FallbackContext {
                        input: input!(self),
                        kind: &kind,
                        _priv: (),
                    }),
                    None => super::fallback::default(&mut FallbackContext {
                        input: input!(self),
                        kind: &kind,
                        _priv: (),
                    }),
                }
            }
            Route::NotFound {
                resources,
                captures,
            } => {
                self.resource_id = None;
                self.captures = captures;
                let kind = FallbackKind::NotFound(resources);
                match self.inner.global_fallback {
                    Some(ref fallback) => fallback.call(&mut FallbackContext {
                        input: input!(self),
                        kind: &kind,
                        _priv: (),
                    }),
                    None => super::fallback::default(&mut FallbackContext {
                        input: input!(self),
                        kind: &kind,
                        _priv: (),
                    }),
                }
            }
        }
    }

    fn process_before_reply(&mut self, output: &mut Output) {
        // append Cookie entries.
        if let Some(ref jar) = self.cookie_jar {
            for cookie in jar.delta() {
                output.headers_mut().append(
                    header::SET_COOKIE,
                    cookie.encoded().to_string().parse().unwrap(),
                );
            }
        }

        // append the value of Content-Length to the response header if missing.
        if let Some(len) = output.body().content_length() {
            output
                .headers_mut()
                .entry(header::CONTENT_LENGTH)
                .expect("never fails")
                .or_insert_with(|| {
                    // safety: '0'-'9' is ascii.
                    // TODO: more efficient
                    unsafe { HeaderValue::from_shared_unchecked(len.to_string().into()) }
                });
        }
    }
}

impl Future for AppFuture {
    type Item = Response<ResponseBody>;
    type Error = Critical;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let polled = loop {
            self.state = match self.state {
                AppFutureState::Init => match self.process_recognize().into_inner() {
                    HandleInner::Ready(result) => break result,
                    HandleInner::PollFn(in_flight) => AppFutureState::InFlight(in_flight),
                },
                AppFutureState::InFlight(ref mut in_flight) => {
                    break ready!((*in_flight)(&mut crate::future::Context::new(input!(self))));
                }
                AppFutureState::Done => panic!("the future has already polled."),
            };
        };
        self.state = AppFutureState::Done;

        let mut output = match polled {
            Ok(output) => output,
            Err(err) => err.into_response(&self.request)?,
        };

        self.process_before_reply(&mut output);

        Ok(Async::Ready(output))
    }
}
