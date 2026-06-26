//! On-disk persistence for generated API specs so they survive restarts.
//!
//! Specs are stored as one JSON file per id under `<MARKIFY_DATA_DIR>/apis`,
//! defaulting to `~/.markify/apis`. All writes are best-effort: a failure to
//! persist never breaks the in-memory flow, it just logs a warning.

use std::collections::HashMap;
use std::path::PathBuf;

use super::spec::ApiSpec;

/// Directory where API specs are stored: `<MARKIFY_DATA_DIR>/apis`, defaulting
/// to `~/.markify/apis`.
pub fn specs_dir() -> PathBuf {
    let base = std::env::var("MARKIFY_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME")
                .or_else(|_| std::env::var("USERPROFILE"))
                .unwrap_or_else(|_| ".".to_string());
            PathBuf::from(home).join(".markify")
        });
    base.join("apis")
}

/// Persist a spec to `<dir>/<id>.json` (best-effort).
pub fn save_spec(spec: &ApiSpec) {
    let dir = specs_dir();
    if let Err(e) = std::fs::create_dir_all(&dir) {
        tracing::warn!(error = %e, "could not create API spec directory");
        return;
    }
    let path = dir.join(format!("{}.json", spec.id));
    match serde_json::to_vec_pretty(spec) {
        Ok(bytes) => {
            if let Err(e) = std::fs::write(&path, bytes) {
                tracing::warn!(error = %e, path = %path.display(), "could not save API spec");
            }
        }
        Err(e) => tracing::warn!(error = %e, "could not serialize API spec"),
    }
}

/// Delete a saved spec (best-effort).
pub fn delete_spec(id: &str) {
    let path = specs_dir().join(format!("{id}.json"));
    let _ = std::fs::remove_file(path);
}

/// Load all saved specs from disk into a map keyed by id.
pub fn load_all() -> HashMap<String, ApiSpec> {
    let mut map = HashMap::new();
    let dir = specs_dir();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return map;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let loaded = std::fs::read(&path)
            .ok()
            .and_then(|b| serde_json::from_slice::<ApiSpec>(&b).ok());
        match loaded {
            Some(spec) => {
                map.insert(spec.id.clone(), spec);
            }
            None => tracing::warn!(path = %path.display(), "could not load API spec"),
        }
    }
    if !map.is_empty() {
        tracing::info!(count = map.len(), "loaded saved API specs from disk");
    }
    map
}
