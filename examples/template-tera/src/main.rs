use {
    crate::support_tera::{Template, WithTera},
    serde::Serialize,
    tsukuyomi::{
        app::config::prelude::*, //
        server::Server,
        App,
    },
};

#[derive(Debug, Serialize)]
struct Index {
    name: String,
}

impl Template for Index {
    fn template_name(&self) -> &str {
        "index.html"
    }
}

fn main() -> tsukuyomi::server::Result<()> {
    let engine = tera::compile_templates!(concat!(env!("CARGO_MANIFEST_DIR"), "/templates/**/*"));

    App::create(
        path!(/{path::param("name")})
            .to(endpoint::any().reply(|name| Index { name }))
            .modify(WithTera::from(engine)),
    ) //
    .map(Server::new)?
    .run()
}

mod support_tera {
    use {
        futures::Poll,
        http::{header::HeaderValue, Response},
        std::sync::Arc,
        tera::Tera,
        tsukuyomi::{
            error::Error,
            handler::{AllowedMethods, Handle, Handler, ModifyHandler},
            input::Input,
        },
    };

    pub trait Template: serde::Serialize {
        fn template_name(&self) -> &str;
        fn extension(&self) -> Option<&str> {
            None
        }
    }

    #[derive(Debug)]
    pub struct WithTera(Arc<Tera>);

    impl From<Tera> for WithTera {
        fn from(engine: Tera) -> Self {
            WithTera(Arc::new(engine))
        }
    }

    impl<H> ModifyHandler<H> for WithTera
    where
        H: Handler,
        H::Output: Template,
    {
        type Output = Response<String>;
        type Handler = WithTeraHandler<H>;

        fn modify(&self, inner: H) -> Self::Handler {
            WithTeraHandler {
                inner,
                engine: self.0.clone(),
            }
        }
    }

    #[derive(Debug)]
    pub struct WithTeraHandler<H> {
        inner: H,
        engine: Arc<Tera>,
    }

    impl<H> Handler for WithTeraHandler<H>
    where
        H: Handler,
        H::Output: Template,
    {
        type Output = Response<String>;
        type Handle = WithTeraHandle<H::Handle>;

        fn allowed_methods(&self) -> Option<&AllowedMethods> {
            self.inner.allowed_methods()
        }

        fn call(&self, input: &mut Input<'_>) -> Self::Handle {
            WithTeraHandle {
                inner: self.inner.call(input),
                engine: self.engine.clone(),
            }
        }
    }

    #[derive(Debug)]
    pub struct WithTeraHandle<H> {
        inner: H,
        engine: Arc<Tera>,
    }

    impl<H> Handle for WithTeraHandle<H>
    where
        H: Handle,
        H::Output: Template,
    {
        type Output = Response<String>;
        type Error = Error;

        fn poll_ready(&mut self, input: &mut Input<'_>) -> Poll<Self::Output, Self::Error> {
            let ctx = futures::try_ready!(self.inner.poll_ready(input).map_err(Into::into));
            let content_type = HeaderValue::from_static(
                ctx.extension()
                    .and_then(mime_guess::get_mime_type_str)
                    .unwrap_or("text/html; charset=utf-8"),
            );
            self.engine
                .render(ctx.template_name(), &ctx)
                .map(|body| {
                    Response::builder()
                        .header("content-type", content_type)
                        .body(body)
                        .expect("should be a valid response")
                        .into()
                })
                .map_err(tsukuyomi::error::internal_server_error)
        }
    }
}
