//! The pages, carried inside the server (no Internet needed in the
//! studio): the journey for the tablet and the phone, the studio screen,
//! the shoots, the live, the programmes and the administration.

use axum::extract::Path;
use axum::http::{header, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use axum::Router;

const INDEX: &str = include_str!("../web/index.html");

/// (name, type, contents).
const FILES: &[(&str, &str, &[u8])] = &[
    (
        "app.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../web/app.js"),
    ),
    (
        "style.css",
        "text/css; charset=utf-8",
        include_bytes!("../web/style.css"),
    ),
    (
        "vendor/preact.umd.js",
        "text/javascript",
        include_bytes!("../web/vendor/preact.umd.js"),
    ),
    (
        "vendor/hooks.umd.js",
        "text/javascript",
        include_bytes!("../web/vendor/hooks.umd.js"),
    ),
    (
        "vendor/htm.umd.js",
        "text/javascript",
        include_bytes!("../web/vendor/htm.umd.js"),
    ),
    (
        "fonts/fonts.css",
        "text/css",
        include_bytes!("../web/fonts/fonts.css"),
    ),
    (
        "fonts/bricolage-grotesque.woff2",
        "font/woff2",
        include_bytes!("../web/fonts/bricolage-grotesque.woff2"),
    ),
    (
        "fonts/inter.woff2",
        "font/woff2",
        include_bytes!("../web/fonts/inter.woff2"),
    ),
    (
        "fonts/jetbrains-mono.woff2",
        "font/woff2",
        include_bytes!("../web/fonts/jetbrains-mono.woff2"),
    ),
    (
        "fonts/manrope.woff2",
        "font/woff2",
        include_bytes!("../web/fonts/manrope.woff2"),
    ),
];

async fn index() -> Html<&'static str> {
    Html(INDEX)
}

async fn file(Path(name): Path<String>) -> Response {
    match FILES.iter().find(|(n, _, _)| *n == name) {
        Some((_, mime, body)) => (
            [
                (header::CONTENT_TYPE, *mime),
                (header::CACHE_CONTROL, "no-cache"),
            ],
            *body,
        )
            .into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

pub fn routes<S: Clone + Send + Sync + 'static>() -> Router<S> {
    let mut r = Router::new().route("/static/{*name}", get(file));
    for p in [
        "/",
        "/ecran",
        "/deck",
        "/tournages",
        "/tournages/{id}",
        "/direct",
        "/programmes",
        "/admin",
        "/admin/{page}",
    ] {
        r = r.route(p, get(index));
    }
    r
}
