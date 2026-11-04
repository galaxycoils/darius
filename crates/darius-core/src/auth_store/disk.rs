use super::{AuthStoreError as Error, atomic};
use serde_json::{Value, json};
use std::{fs::File, path::Path};

pub(super) fn load(path: &Path) -> Result<Value, Error> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let file = match options.open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(json!({"version":1,"providers":{}}));
        }
        Err(_) => return Err(Error::Io),
    };
    if !file.metadata().map_err(|_| Error::Io)?.is_file() {
        return Err(Error::InvalidState);
    }
    let value: Value = serde_json::from_reader(file).map_err(|_| Error::InvalidState)?;
    let providers = value["providers"].as_object().ok_or(Error::InvalidState)?;
    if value["version"] != 1 || providers.values().any(|v| !v.is_object()) {
        return Err(Error::InvalidState);
    }
    Ok(value)
}

pub(super) fn update<F>(path: &Path, provider: &str, update: F) -> Result<(), Error>
where
    F: FnOnce(Option<Value>) -> Result<Option<Value>, Error>,
{
    if provider.is_empty() {
        return Err(Error::InvalidState);
    }
    let mut value = load(path)?;
    let providers = value["providers"]
        .as_object_mut()
        .ok_or(Error::InvalidState)?;
    match update(providers.get(provider).cloned())? {
        Some(state) if state.is_object() => {
            providers.insert(provider.into(), state);
        }
        Some(_) => return Err(Error::InvalidState),
        None => {
            providers.remove(provider);
        }
    }
    atomic::replace(path, |file: &mut File| {
        serde_json::to_writer(file, &value).map_err(|_| Error::Io)
    })
}
