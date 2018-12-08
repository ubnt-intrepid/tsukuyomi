//! Components for constructing HTTP applications.

pub mod config;
pub mod fallback;
pub mod route;

mod error;
mod recognizer;
mod service;
mod tree;
mod uri;

#[cfg(test)]
mod tests;

pub use self::{
    config::AppConfig,
    error::{Error, Result},
    fallback::Fallback,
    service::AppService,
};

pub(crate) use self::{recognizer::Captures, uri::CaptureNames};

use {
    self::{
        recognizer::{RecognizeError, Recognizer},
        tree::{Arena, Node, NodeId},
        uri::Uri,
    },
    crate::{core::Never, handler::BoxedHandler, input::body::RequestBody, output::ResponseBody},
    bytes::BytesMut,
    http::{header::HeaderValue, Method, Request, Response},
    indexmap::{IndexMap, IndexSet},
    std::{fmt, sync::Arc},
    tower_service::NewService,
};

/// The main type which represents an HTTP application.
#[derive(Debug, Clone)]
pub struct App {
    inner: Arc<AppInner>,
}

impl App {
    pub fn configure(config: impl AppConfig<()>) -> Result<Self> {
        Self::with_prefix("/", config)
    }

    pub fn with_prefix(prefix: impl AsRef<str>, config: impl AppConfig<()>) -> Result<Self> {
        self::config::configure(prefix, config)
    }
}

impl NewService for App {
    type Request = Request<RequestBody>;
    type Response = Response<ResponseBody>;
    type Error = Never;
    type Service = AppService;
    type InitError = Never;
    type Future = futures01::future::FutureResult<Self::Service, Self::InitError>;

    fn new_service(&self) -> Self::Future {
        futures01::future::ok(AppService {
            inner: self.inner.clone(),
        })
    }
}

#[derive(Debug)]
struct AppInner {
    recognizer: Recognizer<Resource>,
    scopes: Arena<ScopeData>,
}

impl AppInner {
    fn scope(&self, id: NodeId) -> &Node<ScopeData> {
        &self.scopes[id]
    }

    fn resource(&self, id: ResourceId) -> &Resource {
        self.recognizer.get(id.0).expect("the wrong resource ID")
    }

    /// Infers the scope where the input path belongs from the extracted candidates.
    fn infer_scope(&self, path: &str, resources: &[&Resource]) -> &Node<ScopeData> {
        // First, extract a series of common ancestors of candidates.
        let ancestors = {
            let mut ancestors: Option<&[NodeId]> = None;
            for resource in resources {
                let ancestors = ancestors.get_or_insert(&resource.ancestors);
                let n = (*ancestors)
                    .iter()
                    .zip(&resource.ancestors)
                    .position(|(a, b)| a != b)
                    .unwrap_or_else(|| std::cmp::min(ancestors.len(), resource.ancestors.len()));
                *ancestors = &ancestors[..n];
            }
            ancestors
        };

        // Then, find the oldest ancestor that with the input path as the prefix of URI.
        let node_id = ancestors
            .and_then(|ancestors| {
                ancestors
                    .into_iter()
                    .find(|&&scope| self.scope(scope).data.prefix.as_str().starts_with(path)) //
                    .or_else(|| ancestors.last())
                    .cloned()
            })
            .unwrap_or_else(NodeId::root);

        self.scope(node_id)
    }

    fn find_fallback(&self, start: NodeId) -> Option<&(dyn Fallback + Send + Sync + 'static)> {
        let scope = self.scope(start);
        if let Some(ref f) = scope.data.fallback {
            return Some(&**f);
        }
        scope
            .ancestors()
            .into_iter()
            .rev()
            .filter_map(|&id| self.scope(id).data.fallback.as_ref().map(|f| &**f))
            .next()
    }

    fn route(&self, path: &str, method: &Method) -> RouterResult<'_> {
        let mut captures = None;
        let resource = match self.recognizer.recognize(path, &mut captures) {
            Ok(resource) => resource,
            Err(RecognizeError::NotMatched) => {
                return RouterResult::NotFound {
                    resources: vec![],
                    captures,
                    scope: self.scope(NodeId::root()),
                };
            }
            Err(RecognizeError::PartiallyMatched(candidates)) => {
                let resources: Vec<_> = candidates
                    .iter()
                    .filter_map(|i| self.recognizer.get(i))
                    .collect();

                let scope = self.infer_scope(path, &resources);

                return RouterResult::NotFound {
                    resources,
                    captures,
                    scope,
                };
            }
        };

        if let Some(endpoint) = resource.recognize(method) {
            return RouterResult::FoundEndpoint {
                endpoint,
                resource,
                captures,
            };
        }

        let scope = self.scope(resource.scope);

        RouterResult::FoundResource {
            resource,
            captures,
            scope,
        }
    }
}

struct ScopeData {
    prefix: Uri,
    fallback: Option<Box<dyn Fallback + Send + Sync + 'static>>,
}

impl fmt::Debug for ScopeData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ScopeData")
            .field("prefix", &self.prefix)
            .field("fallback", &self.fallback.as_ref().map(|_| "<fallback>"))
            .finish()
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
struct ResourceId(usize);

/// A type representing a set of endpoints with the same HTTP path.
#[derive(Debug)]
pub struct Resource {
    id: ResourceId,
    scope: NodeId,
    ancestors: Vec<NodeId>,
    uri: Uri,
    endpoints: Vec<Endpoint>,
    allowed_methods: IndexMap<Method, usize>,
    allowed_methods_value: HeaderValue,
}

impl Resource {
    pub fn allowed_methods<'a>(&'a self) -> impl Iterator<Item = &'a Method> + 'a {
        self.allowed_methods.keys()
    }

    fn recognize(&self, method: &Method) -> Option<&Endpoint> {
        self.allowed_methods
            .get(method)
            .map(|&pos| &self.endpoints[pos])
    }

    fn update(&mut self) {
        self.allowed_methods_value = {
            let allowed_methods: IndexSet<_> = self
                .allowed_methods
                .keys()
                .chain(Some(&Method::OPTIONS))
                .collect();
            let bytes =
                allowed_methods
                    .iter()
                    .enumerate()
                    .fold(BytesMut::new(), |mut acc, (i, m)| {
                        if i > 0 {
                            acc.extend_from_slice(b", ");
                        }
                        acc.extend_from_slice(m.as_str().as_bytes());
                        acc
                    });
            unsafe { HeaderValue::from_shared_unchecked(bytes.freeze()) }
        };
    }
}

/// A struct representing a set of data associated with an endpoint.
#[doc(hidden)]
pub struct Endpoint {
    id: usize,
    uri: Uri,
    methods: IndexSet<Method>,
    handler: BoxedHandler,
}

#[cfg_attr(tarpaulin, skip)]
impl fmt::Debug for Endpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Endpoint")
            .field("id", &self.id)
            .field("uri", &self.uri)
            .field("methods", &self.methods)
            .finish()
    }
}

#[derive(Debug)]
enum RouterResult<'a> {
    /// The URI is matched and a route associated with the specified method is found.
    FoundEndpoint {
        endpoint: &'a Endpoint,
        resource: &'a Resource,
        captures: Option<Captures>,
    },

    /// the URI is matched, but the method is disallowed.
    FoundResource {
        resource: &'a Resource,
        captures: Option<Captures>,
        scope: &'a Node<ScopeData>,
    },

    /// The URI is not matched to any endpoints.
    NotFound {
        resources: Vec<&'a Resource>,
        captures: Option<Captures>,
        scope: &'a Node<ScopeData>,
    },
}