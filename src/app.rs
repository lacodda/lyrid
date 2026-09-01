use axum::{
    Json, Router,
    extract::State,
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use std::path::{Path, PathBuf};

use serde_json::json;
use sqlx::PgPool;
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    /// Whether session cookies are marked `Secure`. Carried in the state
    /// because it is a property of how this server is reached, and the
    /// handlers that write cookies have no other way to know.
    pub secure_cookie: bool,
}

/// The router, optionally serving the built SPA and the tile pyramid.
///
/// In development Vite serves those and proxies the API here, so `static_dir`
/// is `None`. On a stand this process is the only thing listening, and the
/// difference between those two arrangements is exactly what a stand exists
/// to expose.
pub fn router(pool: PgPool, secure_cookie: bool, static_dir: Option<&Path>) -> Router {
    let api = Router::new()
        .route("/health", get(health))
        .merge(crate::api::artists::routes())
        .merge(crate::api::accounts::routes())
        .with_state(AppState { pool, secure_cookie });

    let Some(root) = static_dir else {
        return api.layer(TraceLayer::new_for_http());
    };

    // Tiles are served on their own, without the SPA fallback: a missing tile
    // must answer 404 so the client can treat it as "no stars here". Falling
    // back to index.html would hand the renderer an HTML page where it
    // expects a binary header -- the trap that broke the first zoom in
    // development, where the dev server does exactly that.
    let tiles = ServeDir::new(root.join("tiles"));

    // Everything else is the SPA: real files when they exist, index.html
    // otherwise, so a deep link into the map loads the app rather than a 404.
    //
    // `ServeDir`'s own `not_found_service` is deliberately not used here: it
    // serves the fallback body but keeps the 404 of the request that missed.
    // A browser renders that fine, so the defect is invisible in development
    // and tells every crawler and uptime monitor that a working page is
    // broken. Routing the miss through the router's fallback instead gives
    // the handler's own 200.
    let index = root.join("index.html");
    let files = ServeDir::new(root);
    let spa = get(move || serve_index(index.clone()));

    api.nest_service("/tiles", tiles)
        .fallback_service(files.fallback(spa))
        .layer(TraceLayer::new_for_http())
}

/// The SPA's entry point, answered with 200 for any client-side route.
async fn serve_index(path: PathBuf) -> Response {
    match tokio::fs::read(&path).await {
        Ok(bytes) => ([(header::CONTENT_TYPE, "text/html; charset=utf-8")], bytes).into_response(),
        Err(error) => {
            tracing::error!(%error, path = %path.display(), "the SPA entry point could not be read");
            (StatusCode::INTERNAL_SERVER_ERROR, "the application could not be loaded").into_response()
        }
    }
}

/// Liveness + readiness in one place: the process answers, and the database
/// round-trip tells whether the server can actually do its job.
async fn health(State(state): State<AppState>) -> Response {
    match sqlx::query("SELECT 1").execute(&state.pool).await {
        Ok(_) => (
            StatusCode::OK,
            Json(json!({
                "status": "ok",
                "version": env!("CARGO_PKG_VERSION"),
                "database": "ok",
            })),
        )
            .into_response(),
        Err(error) => {
            tracing::error!(%error, "health check: database unreachable");
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({
                    "status": "degraded",
                    "version": env!("CARGO_PKG_VERSION"),
                    "database": "unavailable",
                })),
            )
                .into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use axum::{body::Body, http::Request};
    use http_body_util::BodyExt;
    use sqlx::postgres::PgPoolOptions;
    use tower::ServiceExt;

    use super::*;

    /// A pool pointing nowhere: `connect_lazy` never dials until a query runs,
    /// so the router can be exercised without a live database.
    fn dead_pool() -> PgPool {
        PgPoolOptions::new()
            // Keep the failure fast: the default acquire timeout is 30 s.
            .acquire_timeout(std::time::Duration::from_secs(1))
            .connect_lazy("postgres://nobody:nowhere@127.0.0.1:1/lyrid")
            .expect("lazy pool creation does not touch the network")
    }

    /// A directory laid out the way a stand's static root is.
    fn static_root() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("a temporary directory");
        std::fs::write(dir.path().join("index.html"), "<!doctype html><title>lyrid</title>").unwrap();
        std::fs::create_dir_all(dir.path().join("tiles/0/0")).unwrap();
        std::fs::write(
            dir.path().join("tiles/sky.json"),
            r#"{"min_x":-1,"min_y":-1,"max_x":1,"max_y":1,"max_level":0}"#,
        )
        .unwrap();
        std::fs::write(dir.path().join("tiles/0/0/0.bin"), b"LYST\x01\x00\x00\x00").unwrap();
        dir
    }

    #[tokio::test]
    async fn a_missing_tile_is_a_404_and_never_the_spa() {
        // The renderer decides "is this a tile?" by the magic bytes because a
        // dev server answers a missing file with 200 and an HTML page. The
        // stand must not repeat that: a tile that does not exist has to say
        // so, or a client that trusts the status draws a web page as stars.
        let dir = static_root();
        let app = router(dead_pool(), false, Some(dir.path()));

        let response = app.oneshot(Request::get("/tiles/9/9/9.bin").body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert!(
            !body.starts_with(b"<!doctype"),
            "a missing tile answered with the SPA: {:?}",
            String::from_utf8_lossy(&body)
        );
    }

    #[tokio::test]
    async fn an_existing_tile_is_served_as_it_is() {
        let dir = static_root();
        let app = router(dead_pool(), false, Some(dir.path()));

        let response = app.oneshot(Request::get("/tiles/0/0/0.bin").body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(&body[..4], b"LYST", "the tile came back altered");
    }

    #[tokio::test]
    async fn an_unknown_path_falls_back_to_the_spa() {
        // A deep link into the map is a client route, not a file: it has to
        // load the app rather than 404.
        let dir = static_root();
        let app = router(dead_pool(), false, Some(dir.path()));

        let response = app.oneshot(Request::get("/star/54").body(Body::empty()).unwrap()).await.unwrap();
        // 200, not the 404 the miss produced: a crawler or a monitor reads the
        // status, and a working page reporting itself broken is a real defect
        // even though a browser renders it anyway.
        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert!(body.starts_with(b"<!doctype"), "a client route did not get the SPA");
    }

    #[tokio::test]
    async fn the_api_still_answers_when_static_files_are_served() {
        // The fallback must not swallow the routes it sits behind.
        let dir = static_root();
        let app = router(dead_pool(), false, Some(dir.path()));

        let response = app.oneshot(Request::get("/health").body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn without_a_static_directory_nothing_but_the_api_is_served() {
        // Development: Vite owns the SPA, and this process answering with one
        // would mask a misconfigured proxy.
        let app = router(dead_pool(), false, None);
        let response = app.oneshot(Request::get("/index.html").body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn health_reports_degraded_without_a_database() {
        let response = router(dead_pool(), false, None)
            .oneshot(Request::get("/health").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["status"], "degraded");
        assert_eq!(body["database"], "unavailable");
        assert_eq!(body["version"], env!("CARGO_PKG_VERSION"));
    }

    #[tokio::test]
    async fn unknown_routes_return_404() {
        let response = router(dead_pool(), false, None)
            .oneshot(Request::get("/nope").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
