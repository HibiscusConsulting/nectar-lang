use crate::contract_infer::{InferredContract, InferredField, InferredType};

/// Result of verifying an inferred contract against a real API response.
#[derive(Debug)]
pub struct VerificationResult {
    /// Display name for this contract (URL + context)
    pub label: String,
    /// The URL that was called
    pub url: String,
    /// HTTP method
    pub method: String,
    /// Overall status
    pub status: VerificationStatus,
    /// Individual field mismatches (empty on Pass)
    pub mismatches: Vec<Mismatch>,
}

/// Overall verification status.
#[derive(Debug, Clone, PartialEq)]
pub enum VerificationStatus {
    /// All fields match expected types
    Pass,
    /// One or more fields don't match
    Fail,
    /// Could not reach the API endpoint
    Unreachable(String),
    /// Non-2xx HTTP response
    HttpError(u16),
}

/// A single field mismatch between expected and actual.
#[derive(Debug)]
pub struct Mismatch {
    /// Field path, e.g. ["user", "name"]
    pub field_path: Vec<String>,
    /// What we inferred from code usage
    pub expected: InferredType,
    /// What the API actually returned
    pub actual: ActualType,
    /// Human-readable message
    pub message: String,
}

/// What the API actually returned for a field.
#[derive(Debug, Clone, PartialEq)]
pub enum ActualType {
    String,
    Number,
    Bool,
    Array,
    Object,
    Null,
    Missing,
}

impl std::fmt::Display for ActualType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ActualType::String => write!(f, "String"),
            ActualType::Number => write!(f, "Number"),
            ActualType::Bool => write!(f, "Bool"),
            ActualType::Array => write!(f, "Array"),
            ActualType::Object => write!(f, "Object"),
            ActualType::Null => write!(f, "Null"),
            ActualType::Missing => write!(f, "Missing"),
        }
    }
}

/// Verify a set of inferred contracts against a real API.
///
/// `api_base_url` is the staging/canary base URL. For each contract,
/// the fetch URL is resolved relative to this base.
/// `api_token` is an optional Bearer token for authenticated endpoints.
pub fn verify_contracts(
    contracts: &[InferredContract],
    api_base_url: &str,
    api_token: Option<&str>,
) -> Vec<VerificationResult> {
    contracts.iter().map(|c| verify_single(c, api_base_url, api_token)).collect()
}

/// Verify a single contract.
fn verify_single(
    contract: &InferredContract,
    api_base_url: &str,
    api_token: Option<&str>,
) -> VerificationResult {
    let url = resolve_url(&contract.fetch_url, api_base_url);
    let label = format!("{} {} ({})", contract.method, contract.fetch_url, contract.source_context);

    // Make the HTTP request
    let response = make_request(&url, &contract.method, api_token);

    match response {
        Err(e) => VerificationResult {
            label,
            url,
            method: contract.method.clone(),
            status: VerificationStatus::Unreachable(e),
            mismatches: vec![],
        },
        Ok((status_code, body)) => {
            if status_code < 200 || status_code >= 300 {
                return VerificationResult {
                    label,
                    url,
                    method: contract.method.clone(),
                    status: VerificationStatus::HttpError(status_code),
                    mismatches: vec![],
                };
            }

            // Parse JSON
            let json: Result<serde_json::Value, _> = serde_json::from_str(&body);
            match json {
                Err(_) => VerificationResult {
                    label,
                    url,
                    method: contract.method.clone(),
                    status: VerificationStatus::Fail,
                    mismatches: vec![Mismatch {
                        field_path: vec![],
                        expected: InferredType::Object(vec![]),
                        actual: ActualType::String,
                        message: "Response is not valid JSON".to_string(),
                    }],
                },
                Ok(json_value) => {
                    let mut mismatches = Vec::new();
                    verify_fields(&contract.fields, &json_value, &mut mismatches);

                    let status = if mismatches.is_empty() {
                        VerificationStatus::Pass
                    } else {
                        VerificationStatus::Fail
                    };

                    VerificationResult {
                        label,
                        url,
                        method: contract.method.clone(),
                        status,
                        mismatches,
                    }
                }
            }
        }
    }
}

/// Verify inferred fields against a JSON value.
fn verify_fields(
    fields: &[InferredField],
    json: &serde_json::Value,
    mismatches: &mut Vec<Mismatch>,
) {
    for field in fields {
        let actual_value = navigate_json(json, &field.path);

        match actual_value {
            None => {
                mismatches.push(Mismatch {
                    field_path: field.path.clone(),
                    expected: field.inferred_type.clone(),
                    actual: ActualType::Missing,
                    message: format!(
                        "field '{}' not found in API response",
                        field.path.join(".")
                    ),
                });
            }
            Some(value) => {
                let actual_type = json_to_actual_type(value);

                if !types_compatible(&field.inferred_type, &actual_type) {
                    mismatches.push(Mismatch {
                        field_path: field.path.clone(),
                        expected: field.inferred_type.clone(),
                        actual: actual_type,
                        message: format!(
                            "field '{}': expected {}, got {}",
                            field.path.join("."),
                            field.inferred_type,
                            json_to_actual_type(value),
                        ),
                    });
                }
            }
        }
    }
}

/// Navigate a JSON value by field path.
fn navigate_json<'a>(json: &'a serde_json::Value, path: &[String]) -> Option<&'a serde_json::Value> {
    let mut current = json;
    for segment in path {
        match current.get(segment) {
            Some(v) => current = v,
            None => return None,
        }
    }
    Some(current)
}

/// Map a JSON value to our ActualType enum.
fn json_to_actual_type(value: &serde_json::Value) -> ActualType {
    match value {
        serde_json::Value::Null => ActualType::Null,
        serde_json::Value::Bool(_) => ActualType::Bool,
        serde_json::Value::Number(_) => ActualType::Number,
        serde_json::Value::String(_) => ActualType::String,
        serde_json::Value::Array(_) => ActualType::Array,
        serde_json::Value::Object(_) => ActualType::Object,
    }
}

/// Check if an inferred type is compatible with the actual JSON type.
fn types_compatible(expected: &InferredType, actual: &ActualType) -> bool {
    match (expected, actual) {
        // Unknown matches anything — we couldn't determine the expected type
        (InferredType::Unknown, _) => true,
        // Null is a wildcard for nullable fields
        (_, ActualType::Null) => true,
        // Direct matches
        (InferredType::String, ActualType::String) => true,
        (InferredType::Numeric, ActualType::Number) => true,
        (InferredType::Bool, ActualType::Bool) => true,
        (InferredType::Array(_), ActualType::Array) => true,
        (InferredType::Object(_), ActualType::Object) => true,
        // Numeric strings are a common API pattern — allow but warn
        (InferredType::Numeric, ActualType::String) => false,
        // Everything else is a mismatch
        _ => false,
    }
}

/// Resolve a fetch URL relative to the base URL.
fn resolve_url(fetch_url: &str, base_url: &str) -> String {
    if fetch_url.starts_with("http://") || fetch_url.starts_with("https://") {
        fetch_url.to_string()
    } else {
        let base = base_url.trim_end_matches('/');
        let path = if fetch_url.starts_with('/') {
            fetch_url.to_string()
        } else {
            format!("/{}", fetch_url)
        };
        format!("{}{}", base, path)
    }
}

/// Make an HTTP request using a blocking reqwest client.
fn make_request(
    url: &str,
    method: &str,
    token: Option<&str>,
) -> Result<(u16, String), String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("failed to create HTTP client: {}", e))?;

    let mut request = match method {
        "POST" => client.post(url),
        "PUT" => client.put(url),
        "PATCH" => client.patch(url),
        "DELETE" => client.delete(url),
        _ => client.get(url),
    };

    if let Some(tok) = token {
        request = request.bearer_auth(tok);
    }

    request = request.header("Accept", "application/json");

    let response = request.send().map_err(|e| format!("request failed: {}", e))?;
    let status = response.status().as_u16();
    let body = response.text().map_err(|e| format!("failed to read response: {}", e))?;

    Ok((status, body))
}

/// Print verification results to stderr.
pub fn print_verification_results(results: &[VerificationResult]) -> bool {
    let mut all_pass = true;

    for result in results {
        match &result.status {
            VerificationStatus::Pass => {
                eprintln!("[pass] {} {}", result.method, result.url);
            }
            VerificationStatus::Fail => {
                all_pass = false;
                eprintln!("[FAIL] {}", result.label);
                for mismatch in &result.mismatches {
                    eprintln!("  {} — {}", mismatch.field_path.join("."), mismatch.message);
                }
            }
            VerificationStatus::Unreachable(err) => {
                all_pass = false;
                eprintln!("[UNREACHABLE] {} — {}", result.label, err);
            }
            VerificationStatus::HttpError(code) => {
                all_pass = false;
                eprintln!("[HTTP {}] {}", code, result.label);
            }
        }
    }

    all_pass
}

// ==========================================================================
// Tests
// ==========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract_infer::FieldEvidence;
    use crate::token::Span;

    fn dummy_span() -> Span {
        Span::new(0, 0, 1, 1)
    }

    fn make_field(path: &[&str], ty: InferredType) -> InferredField {
        InferredField {
            path: path.iter().map(|s| s.to_string()).collect(),
            inferred_type: ty,
            evidence: vec![FieldEvidence::FieldAccess(
                path.last().unwrap().to_string(),
                dummy_span(),
            )],
        }
    }

    #[test]
    fn test_types_compatible_string() {
        assert!(types_compatible(&InferredType::String, &ActualType::String));
        assert!(!types_compatible(&InferredType::String, &ActualType::Number));
    }

    #[test]
    fn test_types_compatible_numeric() {
        assert!(types_compatible(&InferredType::Numeric, &ActualType::Number));
        assert!(!types_compatible(&InferredType::Numeric, &ActualType::String));
    }

    #[test]
    fn test_types_compatible_bool() {
        assert!(types_compatible(&InferredType::Bool, &ActualType::Bool));
        assert!(!types_compatible(&InferredType::Bool, &ActualType::String));
    }

    #[test]
    fn test_types_compatible_array() {
        assert!(types_compatible(&InferredType::Array(Box::new(InferredType::Unknown)), &ActualType::Array));
        assert!(!types_compatible(&InferredType::Array(Box::new(InferredType::Unknown)), &ActualType::Object));
    }

    #[test]
    fn test_types_compatible_unknown_matches_all() {
        assert!(types_compatible(&InferredType::Unknown, &ActualType::String));
        assert!(types_compatible(&InferredType::Unknown, &ActualType::Number));
        assert!(types_compatible(&InferredType::Unknown, &ActualType::Bool));
        assert!(types_compatible(&InferredType::Unknown, &ActualType::Array));
        assert!(types_compatible(&InferredType::Unknown, &ActualType::Object));
        assert!(types_compatible(&InferredType::Unknown, &ActualType::Null));
    }

    #[test]
    fn test_types_compatible_null_accepted() {
        assert!(types_compatible(&InferredType::String, &ActualType::Null));
        assert!(types_compatible(&InferredType::Numeric, &ActualType::Null));
        assert!(types_compatible(&InferredType::Bool, &ActualType::Null));
    }

    #[test]
    fn test_navigate_json_simple() {
        let json: serde_json::Value = serde_json::json!({
            "name": "Widget",
            "price": 19.99
        });

        let result = navigate_json(&json, &["name".to_string()]);
        assert!(result.is_some());
        assert_eq!(result.unwrap(), &serde_json::json!("Widget"));
    }

    #[test]
    fn test_navigate_json_nested() {
        let json: serde_json::Value = serde_json::json!({
            "user": {
                "profile": {
                    "name": "Blake"
                }
            }
        });

        let result = navigate_json(&json, &["user".to_string(), "profile".to_string(), "name".to_string()]);
        assert!(result.is_some());
        assert_eq!(result.unwrap(), &serde_json::json!("Blake"));
    }

    #[test]
    fn test_navigate_json_missing() {
        let json: serde_json::Value = serde_json::json!({ "name": "Widget" });
        let result = navigate_json(&json, &["price".to_string()]);
        assert!(result.is_none());
    }

    #[test]
    fn test_json_to_actual_type() {
        assert_eq!(json_to_actual_type(&serde_json::json!("hello")), ActualType::String);
        assert_eq!(json_to_actual_type(&serde_json::json!(42)), ActualType::Number);
        assert_eq!(json_to_actual_type(&serde_json::json!(true)), ActualType::Bool);
        assert_eq!(json_to_actual_type(&serde_json::json!([1, 2])), ActualType::Array);
        assert_eq!(json_to_actual_type(&serde_json::json!({"a": 1})), ActualType::Object);
        assert_eq!(json_to_actual_type(&serde_json::Value::Null), ActualType::Null);
    }

    #[test]
    fn test_resolve_url_absolute() {
        assert_eq!(
            resolve_url("https://api.example.com/products", "https://staging.example.com"),
            "https://api.example.com/products"
        );
    }

    #[test]
    fn test_resolve_url_relative() {
        assert_eq!(
            resolve_url("/products/1", "https://staging.example.com"),
            "https://staging.example.com/products/1"
        );
    }

    #[test]
    fn test_resolve_url_no_leading_slash() {
        assert_eq!(
            resolve_url("products/1", "https://staging.example.com"),
            "https://staging.example.com/products/1"
        );
    }

    #[test]
    fn test_resolve_url_trailing_slash() {
        assert_eq!(
            resolve_url("/products", "https://staging.example.com/"),
            "https://staging.example.com/products"
        );
    }

    #[test]
    fn test_verify_fields_all_match() {
        let fields = vec![
            make_field(&["name"], InferredType::String),
            make_field(&["price"], InferredType::Numeric),
            make_field(&["active"], InferredType::Bool),
        ];

        let json = serde_json::json!({
            "name": "Widget",
            "price": 19.99,
            "active": true
        });

        let mut mismatches = Vec::new();
        verify_fields(&fields, &json, &mut mismatches);
        assert!(mismatches.is_empty());
    }

    #[test]
    fn test_verify_fields_missing_field() {
        let fields = vec![
            make_field(&["name"], InferredType::String),
            make_field(&["stock"], InferredType::Numeric),
        ];

        let json = serde_json::json!({
            "name": "Widget"
        });

        let mut mismatches = Vec::new();
        verify_fields(&fields, &json, &mut mismatches);
        assert_eq!(mismatches.len(), 1);
        assert_eq!(mismatches[0].field_path, vec!["stock"]);
        assert_eq!(mismatches[0].actual, ActualType::Missing);
    }

    #[test]
    fn test_verify_fields_wrong_type() {
        let fields = vec![
            make_field(&["price"], InferredType::Numeric),
        ];

        let json = serde_json::json!({
            "price": "19.99"
        });

        let mut mismatches = Vec::new();
        verify_fields(&fields, &json, &mut mismatches);
        assert_eq!(mismatches.len(), 1);
        assert_eq!(mismatches[0].expected, InferredType::Numeric);
        assert_eq!(mismatches[0].actual, ActualType::String);
    }

    #[test]
    fn test_verify_fields_nested() {
        let fields = vec![
            make_field(&["vendor", "name"], InferredType::String),
        ];

        let json = serde_json::json!({
            "vendor": {
                "name": "Acme Corp"
            }
        });

        let mut mismatches = Vec::new();
        verify_fields(&fields, &json, &mut mismatches);
        assert!(mismatches.is_empty());
    }

    #[test]
    fn test_verify_fields_null_accepted() {
        let fields = vec![
            make_field(&["shipping_estimate"], InferredType::String),
        ];

        let json = serde_json::json!({
            "shipping_estimate": null
        });

        let mut mismatches = Vec::new();
        verify_fields(&fields, &json, &mut mismatches);
        assert!(mismatches.is_empty());
    }

    #[test]
    fn test_actual_type_display() {
        assert_eq!(format!("{}", ActualType::String), "String");
        assert_eq!(format!("{}", ActualType::Number), "Number");
        assert_eq!(format!("{}", ActualType::Missing), "Missing");
    }

    #[test]
    fn test_print_verification_all_pass() {
        let results = vec![
            VerificationResult {
                label: "GET /products".to_string(),
                url: "https://api.example.com/products".to_string(),
                method: "GET".to_string(),
                status: VerificationStatus::Pass,
                mismatches: vec![],
            },
        ];
        assert!(print_verification_results(&results));
    }

    #[test]
    fn test_print_verification_with_failures() {
        let results = vec![
            VerificationResult {
                label: "GET /products".to_string(),
                url: "https://api.example.com/products".to_string(),
                method: "GET".to_string(),
                status: VerificationStatus::Fail,
                mismatches: vec![
                    Mismatch {
                        field_path: vec!["price".to_string()],
                        expected: InferredType::Numeric,
                        actual: ActualType::String,
                        message: "expected Numeric, got String".to_string(),
                    },
                ],
            },
        ];
        assert!(!print_verification_results(&results));
    }
}
