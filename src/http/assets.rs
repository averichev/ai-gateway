use axum::{
    http::{StatusCode, Uri, header},
    response::{IntoResponse, Response},
};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "admin/dist/"]
struct AdminAssets;

pub(crate) async fn serve(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let (name, file) = match AdminAssets::get(path) {
        Some(file) => (path, file),
        None if !path.starts_with("api/")
            && !path.starts_with("assets/")
            && !path.contains('.') =>
        {
            (
                "index.html",
                AdminAssets::get("index.html").expect("embedded admin index"),
            )
        }
        None => return StatusCode::NOT_FOUND.into_response(),
    };
    let cache = if name.starts_with("assets/") {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    };
    (
        [
            (
                header::CONTENT_TYPE,
                mime_guess::from_path(name).first_or_octet_stream().as_ref(),
            ),
            (header::CACHE_CONTROL, cache),
        ],
        file.data,
    )
        .into_response()
}
