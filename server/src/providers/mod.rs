pub mod alipay;
pub mod moov;
pub mod unit;

use serde::{Deserialize, Serialize};
use std::fmt;

/// Identifies which payment provider to route to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Alipay,
    Moov,
    Unit,
}

impl fmt::Display for Provider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Provider::Alipay => write!(f, "alipay"),
            Provider::Moov => write!(f, "moov"),
            Provider::Unit => write!(f, "unit"),
        }
    }
}

/// A provider-agnostic payment request (after decryption).
#[derive(Debug, Deserialize)]
pub struct PaymentRequest {
    pub provider: Provider,
    pub action: String,
    #[serde(default)]
    pub amount_cents: u64,
    #[serde(default)]
    pub currency: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    /// Provider-specific fields passed through as-is.
    #[serde(default)]
    pub params: serde_json::Value,
}

/// A provider-agnostic payment response.
#[derive(Debug, Serialize)]
pub struct PaymentResponse {
    pub success: bool,
    pub provider: String,
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl PaymentResponse {
    pub fn ok(provider: &str, action: &str, data: serde_json::Value) -> Self {
        Self {
            success: true,
            provider: provider.to_string(),
            action: action.to_string(),
            data: Some(data),
            error: None,
        }
    }

    pub fn err(provider: &str, action: &str, error: String) -> Self {
        Self {
            success: false,
            provider: provider.to_string(),
            action: action.to_string(),
            data: None,
            error: Some(error),
        }
    }
}

/// Dispatch a payment request to the correct provider.
pub async fn dispatch(
    req: PaymentRequest,
    http: &reqwest::Client,
) -> PaymentResponse {
    match req.provider {
        Provider::Alipay => alipay::handle(req, http).await,
        Provider::Moov => moov::handle(req, http).await,
        Provider::Unit => unit::handle(req, http).await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_deserialize() {
        let p: Provider = serde_json::from_str("\"alipay\"").unwrap();
        assert_eq!(p, Provider::Alipay);
        let p: Provider = serde_json::from_str("\"moov\"").unwrap();
        assert_eq!(p, Provider::Moov);
        let p: Provider = serde_json::from_str("\"unit\"").unwrap();
        assert_eq!(p, Provider::Unit);
    }

    #[test]
    fn test_provider_display() {
        assert_eq!(Provider::Alipay.to_string(), "alipay");
        assert_eq!(Provider::Moov.to_string(), "moov");
        assert_eq!(Provider::Unit.to_string(), "unit");
    }

    #[test]
    fn test_payment_response_ok() {
        let resp = PaymentResponse::ok("alipay", "precreate", serde_json::json!({"qr": "url"}));
        assert!(resp.success);
        assert_eq!(resp.provider, "alipay");
        assert!(resp.data.is_some());
        assert!(resp.error.is_none());
    }

    #[test]
    fn test_payment_response_err() {
        let resp = PaymentResponse::err("moov", "transfer", "insufficient funds".into());
        assert!(!resp.success);
        assert_eq!(resp.provider, "moov");
        assert!(resp.data.is_none());
        assert!(resp.error.is_some());
    }

    #[test]
    fn test_payment_request_deserialize() {
        let json = r#"{
            "provider": "alipay",
            "action": "precreate",
            "amount_cents": 1000,
            "currency": "CNY",
            "description": "Test payment",
            "params": {"out_trade_no": "TX001"}
        }"#;
        let req: PaymentRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.provider, Provider::Alipay);
        assert_eq!(req.action, "precreate");
        assert_eq!(req.amount_cents, 1000);
        assert_eq!(req.currency.as_deref(), Some("CNY"));
    }

    #[test]
    fn test_payment_request_minimal() {
        let json = r#"{"provider": "unit", "action": "balance"}"#;
        let req: PaymentRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.provider, Provider::Unit);
        assert_eq!(req.amount_cents, 0);
        assert!(req.currency.is_none());
        assert!(req.description.is_none());
    }
}
