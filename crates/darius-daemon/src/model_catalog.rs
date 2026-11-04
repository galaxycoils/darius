//! Live model listing with caching; ports Hermes hermes_cli/models_catalog_static.py + agent/models_dev.py.
use darius_core::providers::ProviderProfile;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CatalogSource {
    #[default]
    Fallback,
    Live,
    Cached,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelListing {
    pub id: String,
    pub context_length: Option<u32>,
    pub supports_vision: bool,
    pub supports_tools: bool,
    #[serde(default)]
    pub provider: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CatalogResponse {
    pub source: CatalogSource,
    pub models: Vec<ModelListing>,
}

#[derive(Debug, thiserror::Error)]
pub enum CatalogError {
    #[error("catalog I/O failed")]
    Io,
    #[error("HTTP request failed")]
    Http,
    #[error("provider returned authentication error (401)")]
    Unauthorized,
}
impl From<reqwest::Error> for CatalogError {
    fn from(e: reqwest::Error) -> Self {
        if e.status().map(|s| s.as_u16()) == Some(401) {
            Self::Unauthorized
        } else {
            Self::Http
        }
    }
}

const MEMORY_TTL: Duration = Duration::from_secs(60);

#[derive(Default)]
pub struct MemoryCache {
    entries: HashMap<String, (CatalogResponse, Instant)>,
}
pub struct DiskCache {
    root: PathBuf,
}
impl DiskCache {
    pub fn new(root: &Path) -> Self {
        Self {
            root: root.join("cache/models"),
        }
    }
    fn path(&self, provider: &str) -> PathBuf {
        self.root.join(format!("{}.json", cache_key(provider)))
    }
    pub fn read(&self, provider: &str) -> Option<CatalogResponse> {
        let p = self.path(provider);
        let data = std::fs::read(&p).ok()?;
        match serde_json::from_slice::<CatalogResponse>(&data) {
            Ok(r) if !r.models.is_empty() => Some(r),
            _ => {
                let _ = std::fs::rename(&p, p.with_extension("json.corrupt"));
                None
            }
        }
    }
    pub fn write(&self, provider: &str, resp: &CatalogResponse) -> bool {
        if std::fs::create_dir_all(&self.root).is_err() {
            return false;
        }
        let p = self.path(provider);
        let tmp = p.with_extension("tmp");
        let bytes = match serde_json::to_vec(resp) {
            Ok(b) => b,
            Err(_) => return false,
        };
        if std::fs::write(&tmp, &bytes).is_err() {
            return false;
        }
        std::fs::rename(&tmp, &p).is_ok()
    }
}

pub fn memory_get(mem: &Arc<Mutex<MemoryCache>>, key: &str) -> Option<CatalogResponse> {
    let mut g = mem.lock();
    if let Some((resp, t)) = g.entries.get(key) {
        if t.elapsed() < MEMORY_TTL {
            return Some(resp.clone());
        }
        g.entries.remove(key);
    }
    None
}
pub fn memory_put(mem: &Arc<Mutex<MemoryCache>>, key: String, resp: CatalogResponse) {
    mem.lock().entries.insert(key, (resp, Instant::now()));
}
pub fn cache_key(endpoint: &str) -> String {
    use blake3::Hasher;
    let mut h = Hasher::new();
    h.update(endpoint.as_bytes());
    h.finalize().to_string()
}

pub struct ModelCapabilities {
    pub context_window: u32,
    pub supports_vision: bool,
    pub supports_tools: bool,
}
const MODELS_DEV_URL: &str = "https://models.dev/api.json";
const DISK_TTL: Duration = Duration::from_secs(4 * 3600);
const RETRY_DELAY: Duration = Duration::from_secs(300);

#[derive(Default)]
struct ModelsDevInner {
    cache: Option<(Registry, Instant)>,
    retry_after: Option<Instant>,
}
pub struct ModelsDev {
    home: PathBuf,
    inner: Arc<Mutex<ModelsDevInner>>,
    client: reqwest::Client,
}

#[derive(Clone, Serialize, Deserialize)]
struct RawModel {
    limit: Option<RawLimit>,
    modalities: Option<RawModalities>,
    tool_call: Option<bool>,
    attachment: Option<bool>,
}
#[derive(Clone, Serialize, Deserialize)]
struct RawLimit {
    context: Option<f64>,
}
#[derive(Clone, Serialize, Deserialize)]
struct RawModalities {
    input: Option<Vec<String>>,
}
#[derive(Clone, Serialize, Deserialize)]
struct ProviderEntry {
    models: Option<HashMap<String, RawModel>>,
}
type Registry = HashMap<String, ProviderEntry>;

impl ModelsDev {
    pub fn new(home: &Path) -> Result<Self, CatalogError> {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(10))
            .build()
            .map_err(|_| CatalogError::Http)?;
        Ok(Self {
            home: home.to_path_buf(),
            inner: Arc::new(Mutex::new(ModelsDevInner::default())),
            client,
        })
    }
    fn cache_path(&self) -> PathBuf {
        self.home.join("cache/models/models_dev_cache.json")
    }
    fn etag_path(&self) -> PathBuf {
        self.home.join("cache/models/models_dev_cache.etag")
    }
    fn load_etag(&self) -> Option<String> {
        std::fs::read_to_string(self.etag_path())
            .ok()
            .filter(|s| !s.is_empty())
    }
    fn load_disk_cache(&self) -> Option<Registry> {
        let p = self.cache_path();
        let data = std::fs::read(&p).ok()?;
        let reg: Registry = serde_json::from_slice(&data).ok()?;
        if reg.is_empty() { None } else { Some(reg) }
    }
    fn save_disk_cache(&self, reg: &Registry, etag: Option<String>) {
        if let Some(parent) = self.cache_path().parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let tmp = self.cache_path().with_extension("tmp");
        let bytes = match serde_json::to_vec(reg) {
            Ok(b) => b,
            Err(_) => return,
        };
        if std::fs::write(&tmp, &bytes).is_ok() {
            let _ = std::fs::rename(&tmp, self.cache_path());
        }
        if let Some(etag) = etag {
            let _ = std::fs::write(self.etag_path(), &etag);
        }
    }
    pub async fn lookup_context(&self, provider: &str, model: &str) -> Option<u32> {
        let mdev_id = provider_to_models_dev(provider)?;
        let reg = self.registry().await;
        let models = reg.get(mdev_id)?.models.as_ref()?;
        let entry = models.get(model).or_else(|| {
            models
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(model))
                .map(|(_, v)| v)
        })?;
        let ctx = entry.limit.as_ref()?.context?;
        if ctx > 0.0 { Some(ctx as u32) } else { None }
    }
    #[cfg_attr(test, allow(dead_code))]
    pub async fn lookup_capabilities(
        &self,
        provider: &str,
        model: &str,
    ) -> Option<ModelCapabilities> {
        let mdev_id = provider_to_models_dev(provider)?;
        let reg = self.registry().await;
        let models = reg.get(mdev_id)?.models.as_ref()?;
        let entry = models.get(model).or_else(|| {
            models
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(model))
                .map(|(_, v)| v)
        })?;
        let ctx = entry
            .limit
            .as_ref()
            .and_then(|l| l.context)
            .unwrap_or(200_000.0);
        let vision = entry
            .modalities
            .as_ref()
            .and_then(|m| m.input.as_ref())
            .map(|i| i.contains(&"image".to_string()))
            .unwrap_or(entry.attachment.unwrap_or(false));
        Some(ModelCapabilities {
            context_window: if ctx > 0.0 { ctx as u32 } else { 200_000 },
            supports_vision: vision,
            supports_tools: entry.tool_call.unwrap_or(true),
        })
    }
    async fn registry(&self) -> Registry {
        let backed_off = {
            let mut g = self.inner.lock();
            if let Some((reg, t)) = &g.cache
                && t.elapsed() < DISK_TTL
            {
                return reg.clone();
            }
            if let Some(disk) = self.load_disk_cache() {
                g.cache = Some((disk.clone(), Instant::now()));
                return disk;
            }
            g.retry_after.is_some_and(|t| t > Instant::now())
        };
        if backed_off {
            return Registry::new();
        }
        match self.fetch().await {
            Some(reg) => {
                self.inner.lock().cache = Some((reg.clone(), Instant::now()));
                reg
            }
            None => {
                self.inner.lock().retry_after = Some(Instant::now() + RETRY_DELAY);
                Registry::new()
            }
        }
    }
    async fn fetch(&self) -> Option<Registry> {
        let mut req = self.client.get(MODELS_DEV_URL);
        if let Some(etag) = self.load_etag() {
            req = req.header("If-None-Match", etag);
        }
        let resp = self.client.execute(req.build().ok()?).await.ok()?;
        if resp.status().as_u16() == 304 {
            return self.load_disk_cache();
        }
        if !resp.status().is_success() {
            return None;
        }
        let etag = resp
            .headers()
            .get("ETag")
            .and_then(|v| v.to_str().ok().map(str::to_owned));
        let reg: Registry = resp.json::<Registry>().await.ok()?;
        if reg.is_empty() {
            return None;
        }
        self.save_disk_cache(&reg, etag);
        Some(reg)
    }
}

fn provider_to_models_dev(provider: &str) -> Option<&'static str> {
    match provider {
        "openrouter" => Some("openrouter"),
        "opencode-go" => Some("opencode-go"),
        "opencode-free" => Some("opencode"),
        "kilocode" => Some("kilo"),
        "nvidia" => Some("nvidia"),
        "nous" => Some("nous"),
        _ => None,
    }
}

pub struct CatalogRequest<'a> {
    pub profile: &'a ProviderProfile,
    pub endpoint: &'a str,
    pub bearer_token: Option<&'a str>,
    pub headers: &'a [(&'a str, &'a str)],
    pub context_length: Option<u32>,
}
pub struct ModelCatalog {
    client: reqwest::Client,
    memory: Arc<Mutex<MemoryCache>>,
    disk_root: PathBuf,
    models_dev: ModelsDev,
}

impl ModelCatalog {
    pub fn new(home: impl AsRef<Path>) -> Result<Self, CatalogError> {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|_| CatalogError::Http)?;
        Ok(Self {
            client,
            memory: Arc::new(Mutex::new(MemoryCache::default())),
            disk_root: home.as_ref().to_path_buf(),
            models_dev: ModelsDev::new(home.as_ref())?,
        })
    }
    pub async fn fetch_models(&self, req: &CatalogRequest<'_>) -> CatalogResponse {
        let key = cache_key(req.endpoint);
        if let Some(mut cached) = memory_get(&self.memory, &key) {
            cached.source = CatalogSource::Cached;
            return cached;
        }
        let disk = DiskCache::new(&self.disk_root);
        if let Some(mut cached) = disk.read(&key) {
            cached.source = CatalogSource::Cached;
            memory_put(&self.memory, key, cached.clone());
            return cached;
        }
        match self.fetch_live(req).await {
            Some(mut resp) => {
                resp.source = CatalogSource::Live;
                let _ = disk.write(&key, &resp);
                memory_put(&self.memory, key.clone(), resp.clone());
                resp
            }
            None => CatalogResponse::default(),
        }
    }
    async fn fetch_live(&self, req: &CatalogRequest<'_>) -> Option<CatalogResponse> {
        let mut builder = self.client.get(req.endpoint);
        if let Some(bearer) = req.bearer_token {
            builder = builder.header("authorization", format!("Bearer {bearer}"));
        }
        for (k, v) in req.headers {
            builder = builder.header(*k, *v);
        }
        let resp = match builder.send().await {
            Ok(r) if r.status().as_u16() == 401 => return None,
            Ok(r) if r.status().is_success() => r,
            _ => return None,
        };
        let json: serde_json::Value = resp.json().await.ok()?;
        let data = json["data"].as_array()?;
        Some(CatalogResponse {
            source: CatalogSource::Live,
            models: data
                .iter()
                .filter_map(|entry| {
                    let id = entry["id"].as_str()?.to_string();
                    let cl = entry["context_length"]
                        .as_u64()
                        .and_then(|v| u32::try_from(v).ok());
                    Some(ModelListing {
                        id,
                        context_length: cl,
                        supports_vision: false,
                        supports_tools: false,
                        provider: None,
                    })
                })
                .collect(),
        })
    }
    pub async fn resolve_context_length(
        &self,
        profile: &ProviderProfile,
        model_id: &str,
        req: &CatalogRequest<'_>,
        config_cl: Option<u32>,
    ) -> u32 {
        if let Some(cl) = config_cl {
            return cl;
        }
        let catalog = self.fetch_models(req).await;
        if let Some(m) = catalog.models.iter().find(|m| m.id == model_id)
            && let Some(cl) = m.context_length
        {
            return cl;
        }
        self.models_dev
            .lookup_context(profile.id, model_id)
            .await
            .unwrap_or(128_000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use darius_core::providers::resolve;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{header, method, path},
    };
    const TEST_KEY_ENV: &str = "DARIUS_TEST_CATALOG_KEY";
    fn set_key() {
        unsafe {
            std::env::set_var(TEST_KEY_ENV, "test-catalog-secret");
        }
    }
    fn request(server: &MockServer) -> CatalogRequest<'static> {
        let profile = resolve("nous").unwrap();
        let endpoint = Box::leak(format!("{}/models", server.uri()).into_boxed_str());
        CatalogRequest {
            profile,
            endpoint,
            bearer_token: Some("test-catalog-secret"),
            headers: &[],
            context_length: None,
        }
    }
    fn test_home() -> std::path::PathBuf {
        let h = std::env::temp_dir().join(format!("catalog-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&h).unwrap();
        h
    }

    #[tokio::test]
    async fn live_then_memory_cache_then_disk_no_network() {
        set_key();
        let home = test_home();
        let server = MockServer::start().await;
        Mock::given(method("GET")).and(path("/models")).and(header("authorization", "Bearer test-catalog-secret"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"data":[{"id":"demo-1","context_length":8192},{"id":"demo-2","context_length":16384}]})))
            .expect(1).mount(&server).await;
        let catalog = ModelCatalog::new(&home).unwrap();
        let req = request(&server);
        let first = catalog.fetch_models(&req).await;
        assert_eq!(first.source, CatalogSource::Live);
        assert_eq!(first.models.len(), 2);
        assert_eq!(first.models[0].context_length, Some(8192));
        assert_eq!(
            catalog.fetch_models(&req).await.source,
            CatalogSource::Cached
        );
        let fresh = ModelCatalog::new(&home).unwrap();
        assert_eq!(fresh.fetch_models(&req).await.source, CatalogSource::Cached);
        server.verify().await;
        std::fs::remove_dir_all(home).unwrap();
    }

    #[tokio::test]
    async fn unauthorized_returns_fallback_no_network() {
        let home = test_home();
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/models"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;
        let catalog = ModelCatalog::new(&home).unwrap();
        let req = request(&server);
        let r = catalog.fetch_models(&req).await;
        assert_eq!(r.source, CatalogSource::Fallback);
        assert!(r.models.is_empty());
        std::fs::remove_dir_all(home).unwrap();
    }

    #[tokio::test]
    async fn timeout_returns_fallback() {
        let home = test_home();
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/models"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_delay(Duration::from_secs(30))
                    .set_body_json(serde_json::json!({"data":[]})),
            )
            .mount(&server)
            .await;
        let catalog = ModelCatalog::new(&home).unwrap();
        let req = request(&server);
        let r = catalog.fetch_models(&req).await;
        assert_eq!(r.source, CatalogSource::Fallback);
        std::fs::remove_dir_all(home).unwrap();
    }

    #[tokio::test]
    async fn corrupt_disk_cache_quarantined() {
        let home = test_home();
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/models"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"data":[{"id":"ok","context_length":4096}]})),
            )
            .mount(&server)
            .await;
        let catalog = ModelCatalog::new(&home).unwrap();
        let req = request(&server);
        catalog.fetch_models(&req).await;
        let cf = std::fs::read_dir(home.join("cache/models"))
            .unwrap()
            .find(|e| {
                e.as_ref()
                    .unwrap()
                    .path()
                    .extension()
                    .and_then(|x| x.to_str())
                    == Some("json")
            })
            .unwrap()
            .unwrap()
            .path();
        std::fs::write(&cf, b"not json").unwrap();
        let catalog2 = ModelCatalog::new(&home).unwrap();
        let r = catalog2.fetch_models(&req).await;
        assert_eq!(r.source, CatalogSource::Live);
        assert!(cf.with_extension("json.corrupt").exists());
        std::fs::remove_dir_all(home).unwrap();
    }

    #[tokio::test]
    async fn context_chain_config_wins_over_models_dev() {
        let home = test_home();
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/models"))
            .respond_with(ResponseTemplate::new(200).set_body_json(
                serde_json::json!({"data":[{"id":"hermes-3-405b","context_length":131072}]}),
            ))
            .mount(&server)
            .await;
        let catalog = ModelCatalog::new(&home).unwrap();
        let req = request(&server);
        let profile = resolve("nous").unwrap();
        assert_eq!(
            catalog
                .resolve_context_length(profile, "hermes-3-405b", &req, Some(999))
                .await,
            999
        );
        assert_eq!(
            catalog
                .resolve_context_length(profile, "hermes-3-405b", &req, None)
                .await,
            131072
        );
        std::fs::remove_dir_all(home).unwrap();
    }

    #[tokio::test]
    async fn context_chain_falls_through_to_default_no_network() {
        let home = test_home();
        let catalog = ModelCatalog::new(&home).unwrap();
        let profile = resolve("nous").unwrap();
        let req = CatalogRequest {
            profile: resolve("opencode-free").unwrap(),
            endpoint: "http://127.0.0.1:1/invalid",
            bearer_token: None,
            headers: &[],
            context_length: None,
        };
        let ctx = catalog
            .resolve_context_length(profile, "unknown-model", &req, None)
            .await;
        assert_eq!(ctx, 128_000);
        std::fs::remove_dir_all(home).unwrap();
    }
}
