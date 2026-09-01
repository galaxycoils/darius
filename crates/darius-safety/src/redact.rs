//! Secret redaction utilities for logs and tool previews.

use regex::Regex;
use std::sync::LazyLock;

static SECRET_PATTERNS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    vec![
        // OpenAI / general sk- keys: sk-...
        Regex::new(r"(?i)\bsk-[a-zA-Z0-9_\-]{16,}\b").unwrap(),
        // Generic api_key assignment or json: "api_key": "...", api_key = "..."
        Regex::new(r#"(?i)(["']?api[_-]?key["']?\s*[:=]\s*["']?)([a-zA-Z0-9_\-]{8,})(["']?)"#).unwrap(),
        // Bearer tokens: Bearer ...
        Regex::new(r#"(?i)(bearer\s+)([a-zA-Z0-9_\-\.]{12,})"#).unwrap(),
        // Generic token / secret / password
        Regex::new(r#"(?i)(["']?(?:secret|token|password|auth_token)["']?\s*[:=]\s*["']?)([a-zA-Z0-9_\-]{8,})(["']?)"#).unwrap(),
    ]
});

/// Redact known secret patterns (sk- keys, Bearer tokens, api_key fields) in text.
pub fn redact_secrets(input: &str) -> String {
    let mut result = input.to_string();

    // 1. sk- keys
    let sk_re = &SECRET_PATTERNS[0];
    result = sk_re.replace_all(&result, "sk-[REDACTED]").to_string();

    // 2. api_key
    let apikey_re = &SECRET_PATTERNS[1];
    result = apikey_re
        .replace_all(&result, "${1}[REDACTED]${3}")
        .to_string();

    // 3. Bearer
    let bearer_re = &SECRET_PATTERNS[2];
    result = bearer_re.replace_all(&result, "${1}[REDACTED]").to_string();

    // 4. token/password/secret
    let token_re = &SECRET_PATTERNS[3];
    result = token_re
        .replace_all(&result, "${1}[REDACTED]${3}")
        .to_string();

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_redact_sk_keys() {
        let input = "export DARIUS_API_KEY=sk-proj1234567890abcdef1234 and continue";
        let redacted = redact_secrets(input);
        assert!(!redacted.contains("sk-proj1234567890abcdef1234"));
        assert!(redacted.contains("sk-[REDACTED]"));
    }

    #[test]
    fn test_redact_bearer_token() {
        let input = "Authorization: Bearer mysecretbearertoken123456789";
        let redacted = redact_secrets(input);
        assert!(!redacted.contains("mysecretbearertoken123456789"));
        assert!(redacted.contains("Bearer [REDACTED]"));
    }

    #[test]
    fn test_redact_api_key_json() {
        let input = r#"{"api_key": "topsecretapikey123", "status": "ok"}"#;
        let redacted = redact_secrets(input);
        assert!(!redacted.contains("topsecretapikey123"));
        assert!(redacted.contains(r#"{"api_key": "[REDACTED]""#));
    }

    #[test]
    fn test_clean_text_unchanged() {
        let input = "Hello world, this is a normal response with no secrets.";
        assert_eq!(redact_secrets(input), input);
    }
}
