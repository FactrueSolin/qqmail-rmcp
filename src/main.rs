mod config;
mod error;
mod mail;
mod mcp;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Redirect, Response};
use axum::{Json, extract::Query, extract::State, routing::get};
use mail::oauth::{
    LocalOAuthStateStore, OAuthCallbackQuery, account_id_from_state, complete_local_oauth_callback,
};
use mcp::QqMailServer;
use rmcp::transport::streamable_http_server::StreamableHttpService;
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use serde::Deserialize;
use std::sync::Arc;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,qqmail_rmcp=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let config = config::AppConfig::from_env().map_err(|e| {
        tracing::error!("Configuration error: {}", e);
        e
    })?;

    let bind_addr = config.mcp_bind;
    let token = config.mcp_access_token.clone();

    tracing::info!("Starting qqmail-rmcp server");
    tracing::info!("Listening on {}", bind_addr);
    tracing::info!("MCP route: /mcp");
    tracing::info!(
        "Configured QQ account ids: {}",
        config
            .accounts
            .keys()
            .cloned()
            .collect::<Vec<_>>()
            .join(", ")
    );

    let config = Arc::new(config);
    let oauth_states = Arc::new(LocalOAuthStateStore::default());

    let service = StreamableHttpService::new(
        {
            let config = config.clone();
            move || Ok(QqMailServer::new(config.clone()))
        },
        LocalSessionManager::default().into(),
        Default::default(),
    );

    let state = Arc::new(HttpState {
        config: config.clone(),
        oauth_states,
    });

    let mcp_service_router = Router::new().nest_service("/mcp", service).route_layer(
        axum::middleware::from_fn_with_state(token.clone(), auth_middleware),
    );
    let mcp_router = Router::new()
        .merge(mcp_service_router)
        .route(
            "/oauth/start",
            get(oauth_start).route_layer(axum::middleware::from_fn_with_state(
                token.clone(),
                auth_middleware,
            )),
        )
        .route("/oauth/callback", get(oauth_callback))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(bind_addr).await?;

    tracing::info!("Server ready at http://{}/mcp", bind_addr);

    axum::serve(listener, mcp_router)
        .with_graceful_shutdown(async {
            tokio::signal::ctrl_c().await.unwrap();
            tracing::info!("Shutting down...");
        })
        .await?;

    Ok(())
}

struct HttpState {
    config: Arc<config::AppConfig>,
    oauth_states: Arc<LocalOAuthStateStore>,
}

#[derive(Deserialize)]
struct OAuthStartQuery {
    account: String,
}

async fn oauth_start(
    State(state): State<Arc<HttpState>>,
    Query(query): Query<OAuthStartQuery>,
) -> Response {
    let Some(account) = state.config.account(query.account.trim()) else {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "account_not_found" })),
        )
            .into_response();
    };
    match state.oauth_states.authorization_url(account) {
        Ok(url) => Redirect::temporary(&url).into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "oauth_start_failed", "message": e.to_string() })),
        )
            .into_response(),
    }
}

async fn oauth_callback(
    State(state): State<Arc<HttpState>>,
    Query(query): Query<OAuthCallbackQuery>,
) -> Response {
    let Ok(account_id) = account_id_from_state(&query.state) else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "invalid_oauth_state" })),
        )
            .into_response();
    };
    let Some(account) = state.config.account(&account_id) else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "invalid_oauth_state" })),
        )
            .into_response();
    };
    let callback = OAuthCallbackQuery {
        state: query.state,
        code: query.code,
        error: query.error,
    };
    match complete_local_oauth_callback(
        &state.oauth_states,
        state.config.token_store_path.clone(),
        account,
        callback,
    )
    .await
    {
        Ok(token) => Json(serde_json::json!({
            "status": "authorized",
            "account": token.account_id,
            "provider": token.provider,
            "scopes": token.scopes,
        }))
        .into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "oauth_callback_failed", "message": e.to_string() })),
        )
            .into_response(),
    }
}

async fn auth_middleware(
    axum::extract::State(token): axum::extract::State<String>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let auth_header = request
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok());

    match auth_header {
        Some(h) if h.starts_with("Bearer ") => {
            let provided = &h[7..];
            if provided == token {
                next.run(request).await
            } else {
                (
                    StatusCode::UNAUTHORIZED,
                    axum::Json(serde_json::json!({
                        "error": "invalid_token",
                        "message": "Invalid or expired access token"
                    })),
                )
                    .into_response()
            }
        }
        _ => (
            StatusCode::UNAUTHORIZED,
            axum::Json(serde_json::json!({
                "error": "missing_token",
                "message": "Authorization header with Bearer token is required"
            })),
        )
            .into_response(),
    }
}
