extern crate either;
extern crate serde;
extern crate tsukuyomi;
extern crate tsukuyomi_session;

use tsukuyomi::app::App;
use tsukuyomi::output::{html, redirect};
use tsukuyomi::route;
use tsukuyomi_session::backend::CookieSessionBackend;
use tsukuyomi_session::Session;

use either::Either;

fn main() {
    let backend = CookieSessionBackend::plain();

    let app =
        App::builder()
            .modifier(tsukuyomi_session::storage(backend))
            .route(
                route!("/") //
                    .with(tsukuyomi_session::extractor())
                    .handle(|session: Session| {
                        session.get::<String>("username").map(|username| {
                            if let Some(username) = username {
                                Either::Right(html(format!(
                                    "Hello, {}! <br />\n\
                                     <form method=\"post\" action=\"/logout\">\n\
                                     <input type=\"submit\" value=\"Log out\" />\n\
                                     </form>\
                                     ",
                                    username
                                )))
                            } else {
                                Either::Left(redirect::to("/login"))
                            }
                        })
                    }),
            ) //
            .route(route!("/login").with(tsukuyomi_session::extractor()).reply(
                |session: Session| {
                    if session.contains("username") {
                        Either::Left(redirect::to("/"))
                    } else {
                        Either::Right(html(
                            "login form\n\
                             <form method=\"post\">\n\
                             <input type=\"text\" name=\"username\">\n\
                             <input type=\"submit\">\n\
                             </form>",
                        ))
                    }
                },
            )) //
            .route(
                route!("/login")
                    .with(tsukuyomi::extractor::body::urlencoded())
                    .with(tsukuyomi_session::extractor())
                    .handle({
                        #[derive(Debug, serde::Deserialize)]
                        struct Form {
                            username: String,
                        }
                        |form: Form, mut session: Session| -> tsukuyomi::error::Result<_> {
                            session.set("username", form.username)?;
                            Ok(redirect::to("/"))
                        }
                    }),
            ) //
            .route(
                route!("/logout")
                    .with(tsukuyomi_session::extractor())
                    .reply(|mut session: Session| {
                        session.remove("username");
                        redirect::to("/")
                    }),
            ) //
            .finish()
            .unwrap();

    tsukuyomi::server(app).run_forever().unwrap();
}
