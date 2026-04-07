use super::{PaymentRequest, PaymentResponse};
use serde_json::json;
use std::env;

const PRODUCTION_BASE: &str = "https://api.moov.io";

/// Load Moov config from environment.
struct MoovConfig {
    api_key: String,
    account_id: String,
    base_url: String,
}

impl MoovConfig {
    fn from_env() -> Result<Self, String> {
        let api_key =
            env::var("MOOV_API_KEY").map_err(|_| "MOOV_API_KEY not set".to_string())?;
        let account_id =
            env::var("MOOV_ACCOUNT_ID").map_err(|_| "MOOV_ACCOUNT_ID not set".to_string())?;
        let base_url = env::var("MOOV_BASE_URL").unwrap_or_else(|_| PRODUCTION_BASE.to_string());

        Ok(Self {
            api_key,
            account_id,
            base_url,
        })
    }
}

/// Make an authenticated GET request to Moov API.
async fn moov_get(
    config: &MoovConfig,
    path: &str,
    http: &reqwest::Client,
) -> Result<serde_json::Value, String> {
    let url = format!("{}{}", config.base_url, path);
    let response = http
        .get(&url)
        .bearer_auth(&config.api_key)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| format!("Moov HTTP error: {}", e))?;

    let status = response.status();
    let body = response
        .text()
        .await
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
    http: &reqwest::Client,
) -> Result<serde_json::Value, String> {
    let url = format!("{}{}", config.base_url, path);
    let response = http
        .post(&url)
        .bearer_auth(&config.api_key)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Moov HTTP error: {}", e))?;

    let status = response.status();
    let body_text = response
        .text()
        .await
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
            match moov_post(&config, &path, body, http).await {
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
            match moov_post(&config, &path, body, http).await {
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
            match moov_get(&config, &path, http).await {
                Ok(data) => PaymentResponse::ok("moov", "get_transfer", data),
                Err(e) => PaymentResponse::err("moov", "get_transfer", e),
            }
        }

        "list_wallets" => {
            let path = format!("/accounts/{}/wallets", config.account_id);
            match moov_get(&config, &path, http).await {
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
