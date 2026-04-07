mod crypto;
mod providers;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};
use tracing::info;

/// Shared application state.
struct AppState {
    key_store: crypto::KeyStore,
    http_client: reqwest::Client,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let state = Arc::new(AppState {
        key_store: crypto::KeyStore::new(300), // 5 minute TTL
        http_client: reqwest::Client::new(),
    });

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .route("/health", get(health))
        .route("/api/key-exchange", post(key_exchange))
        .route("/api/payment", post(payment))
        .route("/api/payment/direct", post(payment_direct))
        .layer(cors)
        .with_state(state);

    let port = std::env::var("PORT").unwrap_or_else(|_| "8080".to_string());
    let addr = format!("0.0.0.0:{}", port);
    info!("Nectar payment server listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

/// GET /health — Cloud Run health check.
async fn health() -> impl IntoResponse {
    Json(serde_json::json!({ "status": "ok" }))
}

/// POST /api/key-exchange — ECDH key exchange.
/// Client sends its X25519 public key, server returns its public key.
/// Both sides derive the shared AES-256 key.
#[derive(Deserialize)]
struct KeyExchangeRequest {
    client_public_key: String,
}

#[derive(Serialize)]
struct KeyExchangeResponse {
    session_id: String,
    server_public_key: String,
}

async fn key_exchange(
    State(state): State<Arc<AppState>>,
    Json(req): Json<KeyExchangeRequest>,
) -> Result<Json<KeyExchangeResponse>, (StatusCode, Json<ErrorResponse>)> {
    let (session_id, server_pub) = state
        .key_store
        .exchange(&req.client_public_key)
        .await
        .map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: format!("key exchange failed: {}", e),
                }),
            )
        })?;

    Ok(Json(KeyExchangeResponse {
        session_id,
        server_public_key: server_pub,
    }))
}

/// POST /api/payment — Encrypted payment request.
/// Wire format: base64-encoded [4-byte ct_len | 12-byte nonce | ciphertext | 64-byte signature]
/// Headers: X-Session-Id (required), X-Verify-Key (optional, base64 Ed25519 public key)
#[derive(Deserialize)]
struct PaymentPayload {
    /// Base64-encoded encrypted wire format.
    data: String,
    /// Session ID from key exchange.
    session_id: String,
    /// Optional: base64 Ed25519 verification key.
    #[serde(default)]
    verify_key: Option<String>,
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

async fn payment(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<PaymentPayload>,
) -> Result<Json<providers::PaymentResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Look up the AES key for this session
    let aes_key = state
        .key_store
        .get_key(&payload.session_id)
        .await
        .ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse {
                    error: "invalid or expired session".to_string(),
                }),
            )
        })?;

    // Parse the encrypted payload
    let encrypted = crypto::EncryptedPayload::from_base64(&payload.data).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: format!("invalid payload: {}", e),
            }),
        )
    })?;

    // Decode optional verification key
    let verify_key_bytes = if let Some(ref vk_b64) = payload.verify_key {
        Some(
            base64::engine::general_purpose::STANDARD
                .decode(vk_b64)
                .map_err(|_| {
                    (
                        StatusCode::BAD_REQUEST,
                        Json(ErrorResponse {
                            error: "invalid verify_key base64".to_string(),
                        }),
                    )
                })?,
        )
    } else {
        None
    };

    // Verify signature and decrypt
    let plaintext = crypto::verify_and_decrypt(
        &encrypted,
        &aes_key,
        verify_key_bytes.as_deref(),
    )
    .map_err(|e| {
        (
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                error: format!("decryption failed: {}", e),
            }),
        )
    })?;

    // Parse the decrypted JSON into a PaymentRequest
    let payment_req: providers::PaymentRequest =
        serde_json::from_slice(&plaintext).map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: format!("invalid payment JSON: {}", e),
                }),
            )
        })?;

    // Dispatch to the correct provider
    let response = providers::dispatch(payment_req, &state.http_client).await;

    Ok(Json(response))
}

use base64::Engine;

/// POST /api/payment/direct — Unencrypted payment request for sandbox/demo mode.
/// Accepts plain JSON PaymentRequest, dispatches directly to provider.
/// In production, use /api/payment with encrypted payload instead.
async fn payment_direct(
    State(state): State<Arc<AppState>>,
    Json(req): Json<providers::PaymentRequest>,
) -> Result<Json<providers::PaymentResponse>, (StatusCode, Json<ErrorResponse>)> {
    info!("Direct payment: provider={}, action={}", req.provider, req.action);
    let response = providers::dispatch(req, &state.http_client).await;
    Ok(Json(response))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use axum::response::Response;
    use tower::ServiceExt;

    fn test_app() -> Router {
        let state = Arc::new(AppState {
            key_store: crypto::KeyStore::new(300),
            http_client: reqwest::Client::new(),
        });

        let cors = CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(Any)
            .allow_headers(Any);

        Router::new()
            .route("/health", get(health))
            .route("/api/key-exchange", post(key_exchange))
            .route("/api/payment", post(payment))
            .layer(cors)
            .with_state(state)
    }

    #[tokio::test]
    async fn test_health_endpoint() {
        let app = test_app();
        let response: Response = app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 1024)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "ok");
    }

    #[tokio::test]
    async fn test_key_exchange_valid() {
        let app = test_app();

        // Generate a client X25519 key pair
        let client_secret =
            x25519_dalek::StaticSecret::random_from_rng(rand::rngs::OsRng);
        let client_pub = x25519_dalek::PublicKey::from(&client_secret);
        let client_pub_b64 =
            base64::engine::general_purpose::STANDARD.encode(client_pub.as_bytes());

        let body = serde_json::json!({
            "client_public_key": client_pub_b64,
        });

        let response: Response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/key-exchange")
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_string(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 1024)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json.get("session_id").is_some());
        assert!(json.get("server_public_key").is_some());
    }

    #[tokio::test]
    async fn test_key_exchange_invalid_key() {
        let app = test_app();

        let body = serde_json::json!({
            "client_public_key": "dG9vc2hvcnQ=",  // "tooshort" — not 32 bytes
        });

        let response: Response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/key-exchange")
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_string(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_payment_invalid_session() {
        let app = test_app();

        let body = serde_json::json!({
            "data": "AAAA",
            "session_id": "nonexistent_session",
        });

        let response: Response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/payment")
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_string(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_payment_full_roundtrip() {
        use aes_gcm::aead::{Aead, KeyInit, OsRng};
        use aes_gcm::{Aes256Gcm, AeadCore};
        use sha2::{Digest, Sha256};

        let state = Arc::new(AppState {
            key_store: crypto::KeyStore::new(300),
            http_client: reqwest::Client::new(),
        });

        // Step 1: Key exchange
        let client_secret =
            x25519_dalek::StaticSecret::random_from_rng(rand::rngs::OsRng);
        let client_pub = x25519_dalek::PublicKey::from(&client_secret);
        let client_pub_b64 =
            base64::engine::general_purpose::STANDARD.encode(client_pub.as_bytes());

        let (session_id, server_pub_b64) =
            state.key_store.exchange(&client_pub_b64).await.unwrap();

        // Client derives the same shared secret
        let server_pub_bytes =
            base64::engine::general_purpose::STANDARD
                .decode(&server_pub_b64)
                .unwrap();
        let mut server_key_arr = [0u8; 32];
        server_key_arr.copy_from_slice(&server_pub_bytes);
        let server_pub = x25519_dalek::PublicKey::from(server_key_arr);
        let shared = client_secret.diffie_hellman(&server_pub);

        // Derive AES key same way as server
        let aes_key = {
            let mut hasher = Sha256::new();
            hasher.update(b"nectar-payment-v1");
            hasher.update(shared.as_bytes());
            let result = hasher.finalize();
            let mut key = [0u8; 32];
            key.copy_from_slice(&result);
            key
        };

        // Step 2: Encrypt a payment request
        let payment_json = serde_json::json!({
            "provider": "moov",
            "action": "list_wallets",
        });
        let plaintext = serde_json::to_vec(&payment_json).unwrap();

        let cipher = Aes256Gcm::new_from_slice(&aes_key).unwrap();
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let ciphertext = cipher.encrypt(&nonce, plaintext.as_ref()).unwrap();

        // Build wire format
        let ct_len = (12 + ciphertext.len()) as u32;
        let mut wire = Vec::new();
        wire.extend_from_slice(&ct_len.to_le_bytes());
        wire.extend_from_slice(nonce.as_slice());
        wire.extend_from_slice(&ciphertext);
        wire.extend_from_slice(&[0u8; 64]); // dummy signature (no verify_key sent)

        let data_b64 =
            base64::engine::general_purpose::STANDARD.encode(&wire);

        // Step 3: Send payment request
        let app = Router::new()
            .route("/api/payment", post(payment))
            .with_state(state);

        let body = serde_json::json!({
            "data": data_b64,
            "session_id": session_id,
        });

        let response: Response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/payment")
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_string(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        // The request will decrypt successfully but the Moov API call will fail
        // (no MOOV_API_KEY set in test). That's expected — we're testing the
        // crypto roundtrip, not the provider API.
        let status = response.status();
        let body = axum::body::to_bytes(response.into_body(), 4096)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        // Should get through decryption and reach the provider (which errors on missing env var)
        assert_eq!(status, StatusCode::OK);
        assert_eq!(json["provider"], "moov");
        assert_eq!(json["action"], "list_wallets");
        // Provider will fail because MOOV_API_KEY is not set, but that proves decryption worked
        assert_eq!(json["success"], false);
        assert!(json["error"]
            .as_str()
            .unwrap()
            .contains("MOOV_API_KEY"));
    }
}
