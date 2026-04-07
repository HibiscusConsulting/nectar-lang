use super::{PaymentRequest, PaymentResponse};
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use chrono::Utc;
use rsa::pkcs8::DecodePrivateKey;
use rsa::sha2::Sha256;
use rsa::signature::SignatureEncoding;
use rsa::{pkcs1v15::SigningKey, RsaPrivateKey};
use serde_json::json;
use std::env;

/// Alipay gateway URLs.
const SANDBOX_GATEWAY: &str = "https://openapi-sandbox.dl.alipaydev.com/gateway.do";
const PRODUCTION_GATEWAY: &str = "https://openapi.alipay.com/gateway.do";

/// Load Alipay config from environment.
struct AlipayConfig {
    app_id: String,
    private_key_pem: String,
    gateway: String,
}

impl AlipayConfig {
    fn from_env() -> Result<Self, String> {
        let app_id = env::var("ALIPAY_APP_ID")
            .map_err(|_| "ALIPAY_APP_ID not set".to_string())?;
        let private_key_pem = env::var("ALIPAY_PRIVATE_KEY")
            .map_err(|_| "ALIPAY_PRIVATE_KEY not set".to_string())?;
        let sandbox = env::var("ALIPAY_SANDBOX")
            .unwrap_or_else(|_| "true".to_string());
        let gateway = if sandbox == "true" {
            SANDBOX_GATEWAY
        } else {
            PRODUCTION_GATEWAY
        };

        Ok(Self {
            app_id,
            private_key_pem,
            gateway: gateway.to_string(),
        })
    }
}

/// Sign the parameter string with RSA2 (SHA256withRSA) per Alipay specification.
fn rsa2_sign(content: &str, private_key_pem: &str) -> Result<String, String> {
    // The env var may contain the raw base64 key or full PEM. Handle both.
    let pem = if private_key_pem.contains("-----BEGIN") {
        private_key_pem.to_string()
    } else {
        format!(
            "-----BEGIN PRIVATE KEY-----\n{}\n-----END PRIVATE KEY-----",
            private_key_pem
        )
    };

    let private_key = RsaPrivateKey::from_pkcs8_pem(&pem)
        .map_err(|e| format!("failed to parse RSA private key: {}", e))?;

    let signing_key = SigningKey::<Sha256>::new(private_key);

    use rsa::signature::Signer;
    let signature = signing_key.sign(content.as_bytes());

    Ok(BASE64.encode(signature.to_bytes()))
}

/// Build the sorted parameter string for signing.
/// Alipay requires parameters sorted alphabetically by key, joined with &,
/// excluding `sign` and `sign_type`.
fn build_sign_content(params: &[(String, String)]) -> String {
    let mut sorted: Vec<_> = params
        .iter()
        .filter(|(k, _)| k != "sign" && k != "sign_type")
        .collect();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    sorted
        .iter()
        .map(|(k, v)| format!("{}={}", k, v))
        .collect::<Vec<_>>()
        .join("&")
}

/// Build common Alipay API parameters.
fn build_common_params(config: &AlipayConfig, method: &str, biz_content: &str) -> Vec<(String, String)> {
    let timestamp = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

    vec![
        ("app_id".into(), config.app_id.clone()),
        ("method".into(), method.into()),
        ("format".into(), "JSON".into()),
        ("charset".into(), "utf-8".into()),
        ("sign_type".into(), "RSA2".into()),
        ("timestamp".into(), timestamp),
        ("version".into(), "1.0".into()),
        ("biz_content".into(), biz_content.into()),
    ]
}

/// Execute an Alipay API call.
async fn call_alipay(
    config: &AlipayConfig,
    method: &str,
    biz_content: serde_json::Value,
    http: &reqwest::Client,
) -> Result<serde_json::Value, String> {
    let biz_str = serde_json::to_string(&biz_content)
        .map_err(|e| format!("JSON serialize error: {}", e))?;

    let mut params = build_common_params(config, method, &biz_str);

    let sign_content = build_sign_content(&params);
    let signature = rsa2_sign(&sign_content, &config.private_key_pem)?;
    params.push(("sign".into(), signature));

    // URL-encode parameters
    let form_params: Vec<(String, String)> = params
        .into_iter()
        .map(|(k, v)| (k, v))
        .collect();

    let response = http
        .post(&config.gateway)
        .form(&form_params)
        .send()
        .await
        .map_err(|e| format!("HTTP request failed: {}", e))?;

    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|e| format!("failed to read response: {}", e))?;

    if !status.is_success() {
        return Err(format!("Alipay returned HTTP {}: {}", status, body));
    }

    serde_json::from_str(&body).map_err(|e| format!("invalid JSON response: {}", e))
}

/// Handle a payment request routed to Alipay.
pub async fn handle(req: PaymentRequest, http: &reqwest::Client) -> PaymentResponse {
    let config = match AlipayConfig::from_env() {
        Ok(c) => c,
        Err(e) => return PaymentResponse::err("alipay", &req.action, e),
    };

    match req.action.as_str() {
        "precreate" => {
            let out_trade_no = req
                .params
                .get("out_trade_no")
                .and_then(|v| v.as_str())
                .unwrap_or("NECTAR_DEMO_001")
                .to_string();

            let amount = format!("{:.2}", req.amount_cents as f64 / 100.0);
            let subject = req
                .description
                .unwrap_or_else(|| "Nectar Payment".to_string());

            let biz = json!({
                "out_trade_no": out_trade_no,
                "total_amount": amount,
                "subject": subject,
            });

            match call_alipay(&config, "alipay.trade.precreate", biz, http).await {
                Ok(data) => PaymentResponse::ok("alipay", "precreate", data),
                Err(e) => PaymentResponse::err("alipay", "precreate", e),
            }
        }

        "query" => {
            let out_trade_no = req
                .params
                .get("out_trade_no")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let trade_no = req
                .params
                .get("trade_no")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let mut biz = serde_json::Map::new();
            if !out_trade_no.is_empty() {
                biz.insert(
                    "out_trade_no".into(),
                    serde_json::Value::String(out_trade_no),
                );
            }
            if !trade_no.is_empty() {
                biz.insert("trade_no".into(), serde_json::Value::String(trade_no));
            }

            match call_alipay(
                &config,
                "alipay.trade.query",
                serde_json::Value::Object(biz),
                http,
            )
            .await
            {
                Ok(data) => PaymentResponse::ok("alipay", "query", data),
                Err(e) => PaymentResponse::err("alipay", "query", e),
            }
        }

        "refund" => {
            let out_trade_no = req
                .params
                .get("out_trade_no")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let refund_amount = req
                .params
                .get("refund_amount")
                .and_then(|v| v.as_str())
                .unwrap_or("0.00")
                .to_string();
            let refund_reason = req
                .params
                .get("refund_reason")
                .and_then(|v| v.as_str())
                .unwrap_or("Demo refund")
                .to_string();

            let biz = json!({
                "out_trade_no": out_trade_no,
                "refund_amount": refund_amount,
                "refund_reason": refund_reason,
            });

            match call_alipay(&config, "alipay.trade.refund", biz, http).await {
                Ok(data) => PaymentResponse::ok("alipay", "refund", data),
                Err(e) => PaymentResponse::err("alipay", "refund", e),
            }
        }

        other => PaymentResponse::err(
            "alipay",
            other,
            format!("unknown Alipay action: {}", other),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_sign_content_sorted() {
        let params = vec![
            ("method".into(), "alipay.trade.precreate".into()),
            ("app_id".into(), "2021000000".into()),
            ("charset".into(), "utf-8".into()),
            ("biz_content".into(), "{}".into()),
        ];
        let content = build_sign_content(&params);
        // Should be sorted alphabetically by key
        assert!(content.starts_with("app_id="));
        assert!(content.contains("&biz_content="));
        assert!(content.contains("&charset="));
        assert!(content.contains("&method="));
    }

    #[test]
    fn test_build_sign_content_excludes_sign() {
        let params = vec![
            ("app_id".into(), "123".into()),
            ("sign".into(), "should_be_excluded".into()),
            ("sign_type".into(), "RSA2".into()),
            ("method".into(), "test".into()),
        ];
        let content = build_sign_content(&params);
        assert!(!content.contains("sign="));
        assert!(!content.contains("sign_type="));
        assert!(content.contains("app_id=123"));
        assert!(content.contains("method=test"));
    }

    #[test]
    fn test_build_common_params() {
        let config = AlipayConfig {
            app_id: "TEST_APP".into(),
            private_key_pem: "".into(),
            gateway: SANDBOX_GATEWAY.into(),
        };
        let params = build_common_params(&config, "alipay.trade.precreate", "{\"a\":1}");
        let keys: Vec<_> = params.iter().map(|(k, _)| k.as_str()).collect();
        assert!(keys.contains(&"app_id"));
        assert!(keys.contains(&"method"));
        assert!(keys.contains(&"format"));
        assert!(keys.contains(&"charset"));
        assert!(keys.contains(&"sign_type"));
        assert!(keys.contains(&"timestamp"));
        assert!(keys.contains(&"version"));
        assert!(keys.contains(&"biz_content"));

        let method = params.iter().find(|(k, _)| k == "method").unwrap();
        assert_eq!(method.1, "alipay.trade.precreate");
    }

    #[test]
    fn test_unknown_action() {
        env::set_var("ALIPAY_APP_ID", "test");
        env::set_var("ALIPAY_PRIVATE_KEY", "test");
        env::set_var("ALIPAY_SANDBOX", "true");
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let req = PaymentRequest {
            provider: super::super::Provider::Alipay,
            action: "unknown_action".into(),
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
            err.contains("unknown Alipay action") || err.contains("ALIPAY_APP_ID"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn test_missing_config() {
        env::remove_var("ALIPAY_APP_ID");
        env::remove_var("ALIPAY_PRIVATE_KEY");
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let req = PaymentRequest {
            provider: super::super::Provider::Alipay,
            action: "precreate".into(),
            amount_cents: 1000,
            currency: Some("CNY".into()),
            description: None,
            params: serde_json::json!({}),
        };
        let http = reqwest::Client::new();
        let resp = rt.block_on(handle(req, &http));
        assert!(!resp.success);
        let err = resp.error.unwrap();
        // env var race: other tests may set/unset these concurrently
        assert!(
            err.contains("not set") || err.contains("ALIPAY") || err.contains("RSA") || err.contains("parse"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn test_gateway_selection() {
        env::set_var("ALIPAY_APP_ID", "test");
        env::set_var("ALIPAY_PRIVATE_KEY", "test");
        env::set_var("ALIPAY_SANDBOX", "true");
        let config = AlipayConfig::from_env().unwrap();
        assert_eq!(config.gateway, SANDBOX_GATEWAY);

        env::set_var("ALIPAY_SANDBOX", "false");
        let config = AlipayConfig::from_env().unwrap();
        assert_eq!(config.gateway, PRODUCTION_GATEWAY);

        // Cleanup
        env::remove_var("ALIPAY_APP_ID");
        env::remove_var("ALIPAY_PRIVATE_KEY");
        env::remove_var("ALIPAY_SANDBOX");
    }
}
