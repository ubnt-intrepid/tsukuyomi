extern crate serde;
extern crate tsukuyomi;
extern crate tsukuyomi_cors;

use {
    serde::{Deserialize, Serialize},
    tsukuyomi::{
        app::{fallback, route, App},
        extractor, Responder,
    },
    tsukuyomi_cors::CORS,
};

#[derive(Debug, Deserialize, Serialize, Responder)]
#[responder(respond_to = "tsukuyomi::output::responder::json")]
struct UserInfo {
    username: String,
    email: String,
    password: String,
    confirm_password: String,
}

fn main() -> tsukuyomi::server::Result<()> {
    let cors = CORS::builder()
        .allow_origin("http://127.0.0.1:5000")?
        .allow_methods(vec!["GET", "POST"])?
        .allow_header("content-type")?
        .max_age(std::time::Duration::from_secs(3600))
        .build();

    App::builder()
        .with(
            route::root()
                .methods("POST")?
                .segment("user")?
                .segment("info")?
                .extract(extractor::body::json())
                .reply(|info: UserInfo| -> tsukuyomi::Result<_> {
                    if info.password != info.confirm_password {
                        return Err(tsukuyomi::error::bad_request(
                            "the field confirm_password is not matched to password.",
                        ));
                    }
                    Ok(info)
                }),
        ) //
        .with(fallback(cors.clone()))
        .modify(cors)
        .build_server()?
        .bind(std::net::SocketAddr::from(([127, 0, 0, 1], 4000)))
        .run()
}
