use {
    askama::Template,
    tsukuyomi::{
        config::prelude::*, //
        App,
        IntoResponse,
    },
    tsukuyomi_server::test::ResponseExt,
};

#[test]
fn test_version_sync() {
    version_sync::assert_html_root_url_updated!("src/lib.rs");
}

#[test]
fn test_template_derivation() -> tsukuyomi_server::Result<()> {
    #[derive(Template, IntoResponse)]
    #[template(source = "Hello, {{ name }}.", ext = "html")]
    #[response(preset = "tsukuyomi_askama::Askama")]
    struct Index {
        name: &'static str,
    }

    let app = App::create(
        path!("/") //
            .to(endpoint::get() //
                .call(|| Index { name: "Alice" })),
    )?;
    let mut server = tsukuyomi_server::test::server(app)?;

    let response = server.perform("/")?;
    assert_eq!(response.status(), 200);
    assert_eq!(response.header("content-type")?, "text/html");
    assert_eq!(response.body().to_utf8()?, "Hello, Alice.");

    Ok(())
}

#[test]
fn test_template_with_modifier() -> tsukuyomi_server::Result<()> {
    #[derive(Template)]
    #[template(source = "Hello, {{ name }}.", ext = "html")]
    struct Index {
        name: &'static str,
    }

    let app = App::create(
        path!("/") //
            .to(endpoint::get() //
                .call(|| Index { name: "Alice" }))
            .modify(tsukuyomi_askama::renderer()),
    )?;
    let mut server = tsukuyomi_server::test::server(app)?;

    let response = server.perform("/")?;
    assert_eq!(response.status(), 200);
    assert_eq!(response.header("content-type")?, "text/html");
    assert_eq!(response.body().to_utf8()?, "Hello, Alice.");

    Ok(())
}
