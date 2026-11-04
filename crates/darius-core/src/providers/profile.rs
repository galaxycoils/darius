#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthType {
    #[default]
    ApiKey,
    Keyless,
    NousOAuth,
}

#[derive(Debug, Clone, Copy)]
pub struct ProviderProfile {
    pub id: &'static str,
    pub aliases: &'static [&'static str],
    pub display_name: &'static str,
    pub base_url: &'static str,
    pub models_url: Option<&'static str>,
    pub key_env: Option<&'static str>,
    pub auth_type: AuthType,
    pub static_headers: &'static [(&'static str, &'static str)],
    pub fallback_models: &'static [&'static str],
    pub default_model: &'static str,
    pub default_max_tokens: Option<u32>,
    pub strict_string_tool_content: bool,
    pub strip_tool_message_names: bool,
    /// Explicit empty Authorization, not an absent header (applied by transport).
    pub omit_authorization: bool,
}

pub(super) const ATTRIBUTION: &[(&str, &str)] = &[
    ("HTTP-Referer", "https://github.com/galaxycoils/darius"),
    ("X-Title", "Darius"),
];

impl ProviderProfile {
    pub fn request_headers(&self) -> Vec<(String, String)> {
        let mut headers: Vec<_> = self
            .static_headers
            .iter()
            .map(|(key, value)| ((*key).into(), (*value).into()))
            .collect();
        headers.push((
            "User-Agent".into(),
            format!("Darius/{}", env!("CARGO_PKG_VERSION")),
        ));
        headers
    }
}
