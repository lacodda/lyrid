use axum::{
    Json, Router,
    extract::State,
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use std::path::{Path, PathBuf};

use serde_json::json;
use sqlx::PgPool;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::trace::TraceLayer;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    /// Whether session cookies are marked `Secure`. Carried in the state
    /// because it is a property of how this server is reached, and the
    /// handlers that write cookies have no other way to know.
    pub secure_cookie: bool,
    /// Where this service is reached from outside, for the links in letters.
    /// A letter is read in a mail client, where there is no page for a
    /// relative link to be relative to.
    pub public_url: String,
    /// How the two letters leave the building, or the log they go to instead.
    pub mailer: crate::mail::Mailer,
    /// The playable stars of the current layout, read once and shared by the
    /// radio and the signal.
    pub dial: crate::api::listening::Dial,
}

/// The router, optionally serving the built SPA and the tile pyramid.
///
/// In development Vite serves those and proxies the API here, so `static_dir`
/// is `None`. On a stand this process is the only thing listening, and the
/// difference between those two arrangements is exactly what a stand exists
/// to expose.
pub fn router(state: AppState, static_dir: Option<&Path>) -> Router {
    // Taken before the state moves into the router: the SPA handler needs it
    // to make the preview image absolute.
    let public_url = state.public_url.clone();

    let api = Router::new()
        .route("/health", get(health))
        .merge(crate::api::artists::routes())
        .merge(crate::api::accounts::routes())
        .merge(crate::api::metrics::routes())
        .merge(crate::api::relations::routes())
        .merge(crate::api::listening::routes())
        .with_state(state);

    let Some(root) = static_dir else {
        return api.layer(TraceLayer::new_for_http());
    };

    // Tiles are served on their own, without the SPA fallback: a missing tile
    // must answer 404 so the client can treat it as "no stars here". Falling
    // back to index.html would hand the renderer an HTML page where it
    // expects a binary header -- the trap that broke the first zoom in
    // development, where the dev server does exactly that.
    //
    // A tile never changes. Rebuilding the sky writes a new layout and a whole
    // new pyramid; there is no such thing as an edited tile, only a replaced
    // sky. So the browser is told it may keep them, and a second visit draws
    // from its own disk rather than asking again.
    //
    // A day, and not `immutable`: both would be promises about the URL, and
    // these URLs are reused by the next pyramid. A day makes the second visit
    // instant and still lets a rebuilt sky reach everyone by the following one.
    let tiles = ServeDir::new(root.join("tiles"));
    let tiles = tower::ServiceBuilder::new()
        .layer(SetResponseHeaderLayer::overriding(
            header::CACHE_CONTROL,
            HeaderValue::from_static("public, max-age=86400"),
        ))
        .service(tiles);

    // `sky.json` is deliberately not cached with them. It is the one file that
    // says which pyramid this is and how deep it goes, so a stale copy sends a
    // fresh browser looking for levels that no longer exist -- the whole sky
    // broken by one held file. It is small, and it is read once per visit.
    //
    // `ServeFile` rather than another `ServeDir`: a route service is handed
    // the whole path, not the remainder after the prefix, so a directory
    // rooted at `tiles/` would go looking for `tiles/tiles/sky.json`.
    let manifest = ServeFile::new(root.join("tiles/sky.json"));
    let manifest = tower::ServiceBuilder::new()
        .layer(SetResponseHeaderLayer::overriding(header::CACHE_CONTROL, HeaderValue::from_static("no-cache")))
        .service(manifest);

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
    let spa = get({
        let base = public_url.clone();
        move || serve_index(index.clone(), base.clone())
    });

    // `/` goes through the same handler as any other client route rather than
    // to `ServeDir`'s directory index. The two would serve the same file with
    // different headers -- and only one of them turns the preview image into
    // an absolute URL, so the root would be the one address whose link unfurls
    // into nothing.
    let files = ServeDir::new(root).append_index_html_on_directories(false);

    api.route_service("/tiles/sky.json", manifest)
        .nest_service("/tiles", tiles)
        .fallback_service(files.fallback(spa))
        .layer(TraceLayer::new_for_http())
}

/// The SPA's entry point, answered with 200 for any client-side route.
///
/// The preview image is made absolute on the way out. A chat or a timeline
/// fetches `og:image` on its own, from its own servers, with no page to
/// resolve a relative path against -- so `/social-preview.png` in the built
/// file becomes `https://…/social-preview.png` here. Done at serve time rather
/// than at build time because only the running server knows what it is reached
/// by: the same bundle is a stand on a home network and a public host.
async fn serve_index(path: PathBuf, public_url: String) -> Response {
    let bytes = match tokio::fs::read(&path).await {
        Ok(bytes) => bytes,
        Err(error) => {
            tracing::error!(%error, path = %path.display(), "the SPA entry point could not be read");
            return (StatusCode::INTERNAL_SERVER_ERROR, "the application could not be loaded").into_response();
        }
    };

    let body = match String::from_utf8(bytes) {
        Ok(html) => absolute_previews(&html, &public_url).into_bytes(),
        // Not valid UTF-8, which index.html always is: served as it is rather
        // than refused, because a page that renders is better than a 500 over
        // a preview tag.
        Err(error) => error.into_bytes(),
    };

    ([(header::CONTENT_TYPE, "text/html; charset=utf-8")], body).into_response()
}

/// Rewrites root-relative preview URLs to absolute ones.
///
/// Only `content="/…"` inside a meta tag, and only the leading slash: a
/// blunter replacement would also rewrite the script and stylesheet the page
/// actually loads, which work perfectly well relative and would break the
/// moment the server is behind a path prefix.
fn absolute_previews(html: &str, public_url: &str) -> String {
    html.replace(r#"content="/"#, &format!(r#"content="{public_url}/"#))
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

    /// The state a router test needs: nothing real behind it, because these
    /// tests are about routing rather than about what the handlers do.
    fn state() -> AppState {
        AppState {
            pool: dead_pool(),
            secure_cookie: false,
            public_url: "http://localhost:8080".to_string(),
            mailer: crate::mail::Mailer::Log,
            dial: crate::api::listening::Dial::default(),
        }
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
        let app = router(state(), Some(dir.path()));

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
        let app = router(state(), Some(dir.path()));

        let response = app.oneshot(Request::get("/tiles/0/0/0.bin").body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(&body[..4], b"LYST", "the tile came back altered");
    }

    #[tokio::test]
    async fn the_preview_image_is_absolute_by_the_time_it_leaves() {
        // A chat fetches og:image from its own servers, with no page to
        // resolve a relative path against: a root-relative URL unfurls into
        // nothing at all. The rewrite happens here rather than at build time
        // because the same bundle is a stand and a public host.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("index.html"),
            r#"<!doctype html><meta property="og:image" content="/social-preview.png" /><script src="/assets/app.js"></script>"#,
        )
        .unwrap();

        let response = router(state(), Some(dir.path()))
            .oneshot(Request::get("/star/54").body(Body::empty()).unwrap())
            .await
            .unwrap();
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let html = String::from_utf8_lossy(&body);

        assert!(html.contains(r#"content="http://localhost:8080/social-preview.png""#), "{html}");
        // And nothing else was touched: the script is loaded by the page
        // itself, where relative is correct and absolute would break behind a
        // path prefix.
        assert!(html.contains(r#"src="/assets/app.js""#), "the rewrite reached beyond the preview tags: {html}");
    }

    #[tokio::test]
    async fn the_root_goes_through_the_same_handler_as_any_other_route() {
        // ServeDir would happily serve index.html for `/` from disk, skipping
        // the rewrite -- leaving the root the one address whose link unfurls
        // into nothing, which is also the address people actually share.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("index.html"),
            r#"<!doctype html><meta property="og:image" content="/social-preview.png" />"#,
        )
        .unwrap();

        let response = router(state(), Some(dir.path()))
            .oneshot(Request::get("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let html = String::from_utf8_lossy(&body);
        assert!(
            html.contains(r#"content="http://localhost:8080/social-preview.png""#),
            "the root skipped the rewrite: {html}"
        );
    }

    #[tokio::test]
    async fn a_tile_may_be_kept_by_the_browser() {
        // A tile never changes -- a rebuilt sky is a new pyramid, not an
        // edited one -- so the second visit should draw from disk rather than
        // ask again.
        let dir = static_root();
        let app = router(state(), Some(dir.path()));

        let response = app.oneshot(Request::get("/tiles/0/0/0.bin").body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let cache = response.headers().get(header::CACHE_CONTROL).unwrap().to_str().unwrap();
        assert!(cache.contains("max-age=86400"), "a tile was not cacheable: {cache}");
    }

    #[tokio::test]
    async fn the_sky_manifest_is_never_kept() {
        // The trap this guards: sky.json sits inside /tiles, so it inherits
        // the tiles' caching unless something takes it out. A held copy of it
        // points a fresh browser at levels a rebuilt pyramid no longer has --
        // the whole sky broken by one cached file, for a day, with nothing on
        // screen to explain it.
        let dir = static_root();
        let app = router(state(), Some(dir.path()));

        let response = app.oneshot(Request::get("/tiles/sky.json").body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let cache = response.headers().get(header::CACHE_CONTROL).unwrap().to_str().unwrap();
        assert!(!cache.contains("max-age=86400"), "sky.json inherited the tiles' caching: {cache}");
        assert!(cache.contains("no-cache"), "{cache}");

        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert!(body.starts_with(b"{"), "the manifest route served something else");
    }

    #[tokio::test]
    async fn an_unknown_path_falls_back_to_the_spa() {
        // A deep link into the map is a client route, not a file: it has to
        // load the app rather than 404.
        let dir = static_root();
        let app = router(state(), Some(dir.path()));

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
        let app = router(state(), Some(dir.path()));

        let response = app.oneshot(Request::get("/health").body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn without_a_static_directory_nothing_but_the_api_is_served() {
        // Development: Vite owns the SPA, and this process answering with one
        // would mask a misconfigured proxy.
        let app = router(state(), None);
        let response = app.oneshot(Request::get("/index.html").body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn health_reports_degraded_without_a_database() {
        let response = router(state(), None)
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
        let response = router(state(), None).oneshot(Request::get("/nope").body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
