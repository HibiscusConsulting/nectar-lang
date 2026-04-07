use super::{PaymentRequest, PaymentResponse};
use serde_json::json;
use std::env;

const PRODUCTION_BASE: &str = "https://api.moov.io";

use base64::Engine;

/// Load Moov config from environment.
struct MoovConfig {
    public_key: String,
    secret_key: String,
    account_id: String,
    base_url: String,
}

impl MoovConfig {
    fn from_env() -> Result<Self, String> {
        let public_key =
            env::var("MOOV_API_KEY").map_err(|_| "MOOV_API_KEY not set".to_string())?;
        let secret_key =
            env::var("MOOV_SECRET_KEY").map_err(|_| "MOOV_SECRET_KEY not set".to_string())?;
        let account_id =
            env::var("MOOV_ACCOUNT_ID").map_err(|_| "MOOV_ACCOUNT_ID not set".to_string())?;
        let base_url = env::var("MOOV_BASE_URL").unwrap_or_else(|_| PRODUCTION_BASE.to_string());

        Ok(Self {
            public_key,
            secret_key,
            account_id,
            base_url,
        })
    }
}

/// Get an OAuth bearer token from Moov using Basic auth with public:secret keys.
async fn get_oauth_token(
    config: &MoovConfig,
    scope: &str,
    http: &reqwest::Client,
) -> Result<String, String> {
    let basic = base64::engine::general_purpose::STANDARD
        .encode(format!("{}:{}", config.public_key, config.secret_key));
    let body = format!("grant_type=client_credentials&scope={}", scope);

    let response = http
        .post(&format!("{}/oauth2/token", config.base_url))
        .header("Authorization", format!("Basic {}", basic))
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .await
        .map_err(|e| format!("Moov OAuth error: {}", e))?;

    let status = response.status();
    let text = response.text().await.map_err(|e| format!("OAuth read error: {}", e))?;

    if !status.is_success() {
        return Err(format!("Moov OAuth failed ({}): {}", status, text));
    }

    let json: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| format!("OAuth JSON parse error: {}", e))?;
    json["access_token"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "No access_token in OAuth response".to_string())
}

/// Make an authenticated GET request to Moov API.
async fn moov_get(
    config: &MoovConfig,
    path: &str,
    scope: &str,
    http: &reqwest::Client,
) -> Result<serde_json::Value, String> {
    let token = get_oauth_token(config, scope, http).await?;
    let url = format!("{}{}", config.base_url, path);
    let response = http
        .get(&url)
        .bearer_auth(&token)
        .header("Accept", "application/json")
        .header("Origin", "https://buildnectar.com")
        .send()
        .await
        .map_err(|e| format!("Moov HTTP error: {}", e))?;

    let status = response.status();
    let body = response.text().await
        .map_err(|e| format!("failed to read Moov response: {}", e))?;

    if !status.is_success() {
        return Err(format!("Moov API error (HTTP {}): {}", status, body));
    }

    serde_json::from_str(&body).map_err(|e| format!("invalid Moov JSON: {}", e))
}

/// Make an authenticated POST request to Moov API.
async fn moov_post(
    config: &MoovConfig,
    path: &str,
    body: serde_json::Value,
    scope: &str,
    http: &reqwest::Client,
) -> Result<serde_json::Value, String> {
    let token = get_oauth_token(config, scope, http).await?;
    let url = format!("{}{}", config.base_url, path);
    let response = http
        .post(&url)
        .bearer_auth(&token)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .header("Origin", "https://buildnectar.com")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Moov HTTP error: {}", e))?;

    let status = response.status();
    let body_text = response.text().await
        .map_err(|e| format!("failed to read Moov response: {}", e))?;

    if !status.is_success() {
        return Err(format!("Moov API error (HTTP {}): {}", status, body_text));
    }

    if body_text.is_empty() {
        return Ok(json!({"status": "accepted"}));
    }

    serde_json::from_str(&body_text).map_err(|e| format!("invalid Moov JSON: {}", e))
}

/// Handle a payment request routed to Moov.
pub async fn handle(req: PaymentRequest, http: &reqwest::Client) -> PaymentResponse {
    let config = match MoovConfig::from_env() {
        Ok(c) => c,
        Err(e) => return PaymentResponse::err("moov", &req.action, e),
    };

    match req.action.as_str() {
        "create_payment" => {
            let currency = req.currency.unwrap_or_else(|| "USD".to_string());
            let description = req
                .description
                .unwrap_or_else(|| "Nectar payment".to_string());

            let destination_id = req
                .params
                .get("destination_account_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let body = json!({
                "source": {
                    "accountID": config.account_id,
                },
                "destination": {
                    "accountID": destination_id,
                },
                "amount": {
                    "value": req.amount_cents,
                    "currency": currency,
                },
                "description": description,
            });

            let path = format!("/accounts/{}/transfers", config.account_id);
            let scope = format!("/accounts/{}/transfers.write", config.account_id);
            match moov_post(&config, &path, body, &scope, http).await {
                Ok(data) => PaymentResponse::ok("moov", "create_payment", data),
                Err(e) => PaymentResponse::err("moov", "create_payment", e),
            }
        }

        "create_transfer" => {
            let transfer_type = req
                .params
                .get("transfer_type")
                .and_then(|v| v.as_str())
                .unwrap_or("ach-debit-fund");

            let destination_id = req
                .params
                .get("destination_account_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let currency = req.currency.unwrap_or_else(|| "USD".to_string());
            let description = req
                .description
                .unwrap_or_else(|| "Nectar transfer".to_string());

            let body = json!({
                "source": {
                    "accountID": config.account_id,
                    "paymentMethodID": req.params.get("source_payment_method").and_then(|v| v.as_str()).unwrap_or(""),
                },
                "destination": {
                    "accountID": destination_id,
                    "paymentMethodID": req.params.get("destination_payment_method").and_then(|v| v.as_str()).unwrap_or(""),
                },
                "amount": {
                    "value": req.amount_cents,
                    "currency": currency,
                },
                "facilitatorFee": {},
                "description": description,
                "metadata": {
                    "transfer_type": transfer_type,
                }
            });

            let path = "/transfers".to_string();
            let scope = format!("/accounts/{}/transfers.write", config.account_id);
            match moov_post(&config, &path, body, &scope, http).await {
                Ok(data) => PaymentResponse::ok("moov", "create_transfer", data),
                Err(e) => PaymentResponse::err("moov", "create_transfer", e),
            }
        }

        "get_transfer" => {
            let transfer_id = req
                .params
                .get("transfer_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            if transfer_id.is_empty() {
                return PaymentResponse::err(
                    "moov",
                    "get_transfer",
                    "transfer_id is required".into(),
                );
            }

            let path = format!("/transfers/{}", transfer_id);
            let scope = format!("/accounts/{}/transfers.read", config.account_id);
            match moov_get(&config, &path, &scope, http).await {
                Ok(data) => PaymentResponse::ok("moov", "get_transfer", data),
                Err(e) => PaymentResponse::err("moov", "get_transfer", e),
            }
        }

        "list_accounts" => {
            let path = "/accounts".to_string();
            let scope = "/accounts.read".to_string();
            match moov_get(&config, &path, &scope, http).await {
                Ok(data) => PaymentResponse::ok("moov", "list_accounts", data),
                Err(e) => PaymentResponse::err("moov", "list_accounts", e),
            }
        }

        "list_transfers" => {
            let path = format!("/accounts/{}/transfers", config.account_id);
            let scope = format!("/accounts/{}/transfers.read", config.account_id);
            match moov_get(&config, &path, &scope, http).await {
                Ok(data) => PaymentResponse::ok("moov", "list_transfers", data),
                Err(e) => PaymentResponse::err("moov", "list_transfers", e),
            }
        }

        "list_wallets" => {
            let path = format!("/accounts/{}/wallets", config.account_id);
            let scope = format!("/accounts/{}/wallets.read", config.account_id);
            match moov_get(&config, &path, &scope, http).await {
                Ok(data) => PaymentResponse::ok("moov", "list_wallets", data),
                Err(e) => PaymentResponse::err("moov", "list_wallets", e),
            }
        }

        other => PaymentResponse::err("moov", other, format!("unknown Moov action: {}", other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_missing_config() {
        env::remove_var("MOOV_API_KEY");
        env::remove_var("MOOV_ACCOUNT_ID");
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let req = PaymentRequest {
            provider: super::super::Provider::Moov,
            action: "create_payment".into(),
            amount_cents: 500,
            currency: Some("USD".into()),
            description: None,
            params: serde_json::json!({}),
        };
        let http = reqwest::Client::new();
        let resp = rt.block_on(handle(req, &http));
        assert!(!resp.success);
        let err = resp.error.unwrap();
        assert!(
            err.contains("MOOV_API_KEY") || err.contains("MOOV_ACCOUNT_ID") || err.contains("not set") || err.contains("Moov"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn test_unknown_action() {
        env::set_var("MOOV_API_KEY", "test_key");
        env::set_var("MOOV_ACCOUNT_ID", "test_account");
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let req = PaymentRequest {
            provider: super::super::Provider::Moov,
            action: "invalid_action".into(),
            amount_cents: 0,
            currency: None,
            description: None,
            params: serde_json::Value::Null,
        };
        let http = reqwest::Client::new();
        let resp = rt.block_on(handle(req, &http));
        assert!(!resp.success);
        let err = resp.error.unwrap();
        assert!(
            err.contains("unknown Moov action") || err.contains("MOOV_API_KEY"),
            "unexpected error: {}",
            err
        );

        env::remove_var("MOOV_API_KEY");
        env::remove_var("MOOV_ACCOUNT_ID");
    }

    #[test]
    fn test_get_transfer_missing_id() {
        env::set_var("MOOV_API_KEY", "test_key");
        env::set_var("MOOV_ACCOUNT_ID", "test_account");
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let req = PaymentRequest {
            provider: super::super::Provider::Moov,
            action: "get_transfer".into(),
            amount_cents: 0,
            currency: None,
            description: None,
            params: serde_json::json!({}),
        };
        let http = reqwest::Client::new();
        let resp = rt.block_on(handle(req, &http));
        assert!(!resp.success);
        let err = resp.error.unwrap();
        assert!(
            err.contains("transfer_id is required") || err.contains("MOOV_API_KEY") || err.contains("not set"),
            "unexpected error: {}",
            err
        );

        env::remove_var("MOOV_API_KEY");
        env::remove_var("MOOV_ACCOUNT_ID");
    }

    #[test]
    fn test_config_custom_base_url() {
        env::set_var("MOOV_API_KEY", "k");
        env::set_var("MOOV_ACCOUNT_ID", "a");
        env::set_var("MOOV_BASE_URL", "https://sandbox.moov.io");
        let config = MoovConfig::from_env().unwrap();
        assert_eq!(config.base_url, "https://sandbox.moov.io");

        env::remove_var("MOOV_API_KEY");
        env::remove_var("MOOV_ACCOUNT_ID");
        env::remove_var("MOOV_BASE_URL");
    }

    #[test]
    fn test_config_default_base_url() {
        env::set_var("MOOV_API_KEY", "k");
        env::set_var("MOOV_ACCOUNT_ID", "a");
        env::remove_var("MOOV_BASE_URL");
        let config = MoovConfig::from_env().unwrap();
        assert_eq!(config.base_url, PRODUCTION_BASE);

        env::remove_var("MOOV_API_KEY");
        env::remove_var("MOOV_ACCOUNT_ID");
    }
}
