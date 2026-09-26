use axum::Router;
use axum::extract::Path;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use rust_embed::RustEmbed;

use crate::{Server, content_type};

/// The built web canvas. Run `npm run build` in `web/` before building `wb`.
#[derive(RustEmbed)]
#[folder = "../../web/dist"]
#[allow_missing = true]
struct Assets;

pub fn router() -> Router<Server> {
    Router::new()
        .route("/", get(index))
        .route("/b/{id}", get(index))
        .route("/{*path}", get(asset))
}

async fn index() -> Response {
    file("index.html")
}

async fn asset(Path(path): Path<String>) -> Response {
    file(&path)
}

fn file(path: &str) -> Response {
    match Assets::get(path) {
        Some(file) => ([content_type(path)], file.data.into_owned()).into_response(),
        None if path == "index.html" => (
            StatusCode::SERVICE_UNAVAILABLE,
            "The canvas isn't built into this wb. Run `npm run build` in web/, then rebuild wb.",
        )
            .into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}
