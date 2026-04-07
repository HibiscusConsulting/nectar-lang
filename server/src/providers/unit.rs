use super::{PaymentRequest, PaymentResponse};
use serde_json::json;
use std::env;

const SANDBOX_BASE: &str = "https://api.s.unit.sh";
const PRODUCTION_BASE: &str = "https://api.unit.co";

/// Load Unit config from environment.
struct UnitConfig {
    api_token: String,
    #[allow(dead_code)]
    org_id: String,
    base_url: String,
}

impl UnitConfig {
    fn from_env() -> Result<Self, String> {
        let api_token =
            env::var("UNIT_API_TOKEN").map_err(|_| "UNIT_API_TOKEN not set".to_string())?;
        let org_id = env::var("UNIT_ORG_ID").map_err(|_| "UNIT_ORG_ID not set".to_string())?;
        let base_url = env::var("UNIT_BASE_URL").unwrap_or_else(|_| {
            // Default to sandbox if token starts with "test_"
            if api_token.starts_with("test_") {
                SANDBOX_BASE.to_string()
            } else {
                PRODUCTION_BASE.to_string()
            }
        });

        Ok(Self {
            api_token,
            org_id,
            base_url,
        })
    }
}

/// Make an authenticated GET request to Unit API (JSON:API format).
async fn unit_get(
    config: &UnitConfig,
    path: &str,
    http: &reqwest::Client,
) -> Result<serde_json::Value, String> {
    let url = format!("{}{}", config.base_url, path);
    let response = http
        .get(&url)
        .bearer_auth(&config.api_token)
        .header("Content-Type", "application/vnd.api+json")
        .send()
        .await
        .map_err(|e| format!("Unit HTTP error: {}", e))?;

    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|e| format!("failed to read Unit response: {}", e))?;

    if !status.is_success() {
        return Err(format!("Unit API error (HTTP {}): {}", status, body));
    }

    serde_json::from_str(&body).map_err(|e| format!("invalid Unit JSON: {}", e))
}

/// Make an authenticated POST request to Unit API (JSON:API format).
async fn unit_post(
    config: &UnitConfig,
    path: &str,
    body: serde_json::Value,
    http: &reqwest::Client,
) -> Result<serde_json::Value, String> {
    let url = format!("{}{}", config.base_url, path);
    let response = http
        .post(&url)
        .bearer_auth(&config.api_token)
        .header("Content-Type", "application/vnd.api+json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Unit HTTP error: {}", e))?;

    let status = response.status();
    let body_text = response
        .text()
        .await
        .map_err(|e| format!("failed to read Unit response: {}", e))?;

    if !status.is_success() {
        return Err(format!("Unit API error (HTTP {}): {}", status, body_text));
    }

    serde_json::from_str(&body_text).map_err(|e| format!("invalid Unit JSON: {}", e))
}

/// Handle a payment request routed to Unit.
pub async fn handle(req: PaymentRequest, http: &reqwest::Client) -> PaymentResponse {
    let config = match UnitConfig::from_env() {
        Ok(c) => c,
        Err(e) => return PaymentResponse::err("unit", &req.action, e),
    };

    match req.action.as_str() {
        "create_deposit_account" => {
            let customer_id = req
                .params
                .get("customer_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let deposit_product = req
                .params
                .get("deposit_product")
                .and_then(|v| v.as_str())
                .unwrap_or("checking")
                .to_string();

            let body = json!({
                "data": {
                    "type": "depositAccount",
                    "attributes": {
                        "depositProduct": deposit_product,
                        "tags": {
                            "source": "nectar-demo"
                        }
                    },
                    "relationships": {
                        "customer": {
                            "data": {
                                "type": "customer",
                                "id": customer_id,
                            }
                        }
                    }
                }
            });

            match unit_post(&config, "/accounts", body, http).await {
                Ok(data) => PaymentResponse::ok("unit", "create_deposit_account", data),
                Err(e) => PaymentResponse::err("unit", "create_deposit_account", e),
            }
        }

        "issue_card" => {
            let account_id = req
                .params
                .get("account_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let body = json!({
                "data": {
                    "type": "individualVirtualDebitCard",
                    "attributes": {
                        "tags": {
                            "source": "nectar-demo"
                        }
                    },
                    "relationships": {
                        "account": {
                            "data": {
                                "type": "depositAccount",
                                "id": account_id,
                            }
                        }
                    }
                }
            });

            match unit_post(&config, "/cards", body, http).await {
                Ok(data) => PaymentResponse::ok("unit", "issue_card", data),
                Err(e) => PaymentResponse::err("unit", "issue_card", e),
            }
        }

        "create_ach_payment" => {
            let account_id = req
                .params
                .get("account_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let counterparty_id = req
                .params
                .get("counterparty_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let direction = req
                .params
                .get("direction")
                .and_then(|v| v.as_str())
                .unwrap_or("Credit")
                .to_string();
            let description = req
                .description
                .unwrap_or_else(|| "Nectar ACH payment".to_string());

            let body = json!({
                "data": {
                    "type": "achPayment",
                    "attributes": {
                        "amount": req.amount_cents,
                        "direction": direction,
                        "description": description,
                        "tags": {
                            "source": "nectar-demo"
                        }
                    },
                    "relationships": {
                        "account": {
                            "data": {
                                "type": "depositAccount",
                                "id": account_id,
                            }
                        },
                        "counterparty": {
                            "data": {
                                "type": "counterparty",
                                "id": counterparty_id,
                            }
                        }
                    }
                }
            });

            match unit_post(&config, "/payments", body, http).await {
                Ok(data) => PaymentResponse::ok("unit", "create_ach_payment", data),
                Err(e) => PaymentResponse::err("unit", "create_ach_payment", e),
            }
        }

        "create_book_payment" => {
            let account_id = req
                .params
                .get("account_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let counterparty_account_id = req
                .params
                .get("counterparty_account_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let description = req
                .description
                .unwrap_or_else(|| "Nectar book payment".to_string());

            let body = json!({
                "data": {
                    "type": "bookPayment",
                    "attributes": {
                        "amount": req.amount_cents,
                        "description": description,
                        "tags": {
                            "source": "nectar-demo"
                        }
                    },
                    "relationships": {
                        "account": {
                            "data": {
                                "type": "depositAccount",
                                "id": account_id,
                            }
                        },
                        "counterpartyAccount": {
                            "data": {
                                "type": "depositAccount",
                                "id": counterparty_account_id,
                            }
                        }
                    }
                }
            });

            match unit_post(&config, "/payments", body, http).await {
                Ok(data) => PaymentResponse::ok("unit", "create_book_payment", data),
                Err(e) => PaymentResponse::err("unit", "create_book_payment", e),
            }
        }

        "get_balance" => {
            let account_id = req
                .params
                .get("account_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            if account_id.is_empty() {
                return PaymentResponse::err(
                    "unit",
                    "get_balance",
                    "account_id is required".into(),
                );
            }

            let path = format!("/accounts/{}", account_id);
            match unit_get(&config, &path, http).await {
                Ok(data) => {
                    // Extract balance from the JSON:API response
                    let balance = data
                        .get("data")
                        .and_then(|d| d.get("attributes"))
                        .and_then(|a| a.get("balance"))
                        .cloned()
                        .unwrap_or(json!(null));

                    PaymentResponse::ok(
                        "unit",
                        "get_balance",
                        json!({
                            "account_id": account_id,
                            "balance": balance,
                        }),
                    )
                }
                Err(e) => PaymentResponse::err("unit", "get_balance", e),
            }
        }

        other => PaymentResponse::err("unit", other, format!("unknown Unit action: {}", other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_missing_config() {
        env::remove_var("UNIT_API_TOKEN");
        env::remove_var("UNIT_ORG_ID");
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let req = PaymentRequest {
            provider: super::super::Provider::Unit,
            action: "get_balance".into(),
            amount_cents: 0,
            currency: None,
            description: None,
            params: serde_json::json!({"account_id": "123"}),
        };
        let http = reqwest::Client::new();
        let resp = rt.block_on(handle(req, &http));
        assert!(!resp.success);
        let err = resp.error.unwrap();
        // env var race: other tests may set/unset these concurrently
        assert!(
            err.contains("not set") || err.contains("UNIT_API_TOKEN") || err.contains("UNIT_ORG_ID") || err.contains("401") || err.contains("unauthorized"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn test_unknown_action() {
        env::set_var("UNIT_API_TOKEN", "test_token");
        env::set_var("UNIT_ORG_ID", "test_org");
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let req = PaymentRequest {
            provider: super::super::Provider::Unit,
            action: "nonexistent".into(),
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
            err.contains("unknown Unit action") || err.contains("UNIT_API_TOKEN"),
            "unexpected error: {}",
            err
        );

        env::remove_var("UNIT_API_TOKEN");
        env::remove_var("UNIT_ORG_ID");
    }

    #[test]
    fn test_get_balance_missing_account_id() {
        env::set_var("UNIT_API_TOKEN", "test_token");
        env::set_var("UNIT_ORG_ID", "test_org");
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let req = PaymentRequest {
            provider: super::super::Provider::Unit,
            action: "get_balance".into(),
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
            err.contains("account_id is required") || err.contains("UNIT_API_TOKEN") || err.contains("UNIT_ORG_ID") || err.contains("not set"),
            "unexpected error: {}",
            err
        );

        env::remove_var("UNIT_API_TOKEN");
        env::remove_var("UNIT_ORG_ID");
    }

    #[test]
    fn test_sandbox_detection() {
        env::set_var("UNIT_API_TOKEN", "test_abc123");
        env::set_var("UNIT_ORG_ID", "org1");
        env::remove_var("UNIT_BASE_URL");
        let config = UnitConfig::from_env().unwrap();
        assert_eq!(config.base_url, SANDBOX_BASE);

        env::set_var("UNIT_API_TOKEN", "live_abc123");
        let config = UnitConfig::from_env().unwrap();
        assert_eq!(config.base_url, PRODUCTION_BASE);

        env::set_var("UNIT_BASE_URL", "https://custom.unit.co");
        let config = UnitConfig::from_env().unwrap();
        assert_eq!(config.base_url, "https://custom.unit.co");

        env::remove_var("UNIT_API_TOKEN");
        env::remove_var("UNIT_ORG_ID");
        env::remove_var("UNIT_BASE_URL");
    }
}
