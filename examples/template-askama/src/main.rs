extern crate tsukuyomi;
#[macro_use]
extern crate askama;
extern crate failure;
extern crate http;

use askama::Template as _Template;
use failure::SyncFailure;
use http::{header, Response};
use tsukuyomi::output::{Output, Responder};
use tsukuyomi::{App, Error, Input};

struct Template<T: _Template>(T);

impl<T: _Template> Responder for Template<T> {
    fn respond_to(self, _: &Input) -> Result<Output, Error> {
        let body = self.0
            .render()
            .map_err(|e| Error::internal_server_error(SyncFailure::new(e)))?;
        Response::builder()
            .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
            .body(body)
            .map(Into::into)
            .map_err(Error::internal_server_error)
    }
}

#[derive(Debug, Template)]
#[template(path = "index.html")]
struct Hello {
    name: String,
}

fn index(_: &mut Input) -> Template<Hello> {
    let name = Input::with(|input| input.params()[0].to_owned());
    Template(Hello { name: name })
}

fn main() -> tsukuyomi::AppResult<()> {
    let app = App::builder()
        .mount("/", |r| {
            r.get("/:name").handle(index);
        })
        .finish()?;

    tsukuyomi::run(app)
}