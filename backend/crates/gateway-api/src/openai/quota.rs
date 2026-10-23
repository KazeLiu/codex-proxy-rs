//! Client Key 鉴权后的额度插件入口，未被插件接管时不提供额度结果

use axum::{
    Router,
    extract::State,
    http::{HeaderMap, HeaderValue, StatusCode, header},
    middleware,
    response::{IntoResponse, Response},
    routing::get,
};

use super::{
    auth::{client_access_error_response, verify_client},
    error::openai_error_response,
    middleware::{self as plugin_middleware, RequestInput},
};
use crate::ApiState;

pub(super) fn router() -> Router<ApiState> {
    Router::new()
        .route(super::router::QUOTA_PATH, get(quota))
        .layer(middleware::map_response(no_store))
}

async fn quota(
    State(state): State<ApiState>,
    headers: HeaderMap,
    uri: axum::http::Uri,
) -> Response {
    let client = match verify_client(state.openai(), &headers).await {
        Ok(client) => client,
        Err(error) => return client_access_error_response(error.into()),
    };
    let invalid_body = headers.contains_key(header::TRANSFER_ENCODING)
        || headers
            .get(header::CONTENT_LENGTH)
            .is_some_and(|value| value.as_bytes() != b"0");
    if uri.query().is_some() || invalid_body {
        return openai_error_response(
            StatusCode::BAD_REQUEST,
            "Quota query does not accept query parameters or a body",
            "invalid_request_error",
            "invalid_quota_query",
        )
        .into_response();
    }
    plugin_middleware::query_response(
        state.openai().execution(),
        client,
        RequestInput::query(super::router::QUOTA_PATH.to_owned(), None, &headers),
        |_| async {
            openai_error_response(
                StatusCode::NOT_FOUND,
                "No quota plugin handles this request",
                "invalid_request_error",
                "quota_plugin_unavailable",
            )
            .into_response()
        },
    )
    .await
}

async fn no_store(mut response: Response) -> Response {
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}
