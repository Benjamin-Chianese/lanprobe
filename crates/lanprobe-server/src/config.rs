//! Snapshot unique de la config frontend (settings, profils réseau,
//! profils portscan) détenu côté backend et synchronisé via le bus
//! d'events `config:update`. C'est ce qui permet à un client web et au
//! desktop Tauri de rester en phase : toute modif sur l'un est push sur
//! l'autre à travers `broadcast::Sender<BroadcastEvent>`.
//!
//! Migration : la version précédente stockait la config dans
//! `<data_dir>/lanprobe/config.json` via tauri-plugin-store. Au premier
//! lancement, si on trouve l'ancien fichier mais pas le nouveau, on le
//! recopie pour ne pas perdre les profils/settings de l'utilisateur.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde_json::Value;

pub struct ConfigStore {
    path: PathBuf,
    data: Mutex<Value>,
}

impl ConfigStore {
    pub fn load(path: PathBuf) -> Self {
        if !path.exists() {
            if let Some(old_dir) = dirs_next::data_dir() {
                let old = old_dir.join("lanprobe").join("config.json");
                if old.exists() {
                    if let Ok(raw) = std::fs::read_to_string(&old) {
                        if let Some(parent) = path.parent() {
                            let _ = std::fs::create_dir_all(parent);
                        }
                        let _ = std::fs::write(&path, &raw);
                    }
                }
            }
        }
        let data: Value = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_else(|| serde_json::json!({}));
        Self {
            path,
            data: Mutex::new(data),
        }
    }

    pub fn get(&self) -> Value {
        self.data
            .lock()
            .map(|g| g.clone())
            .unwrap_or(Value::Null)
    }

    pub fn put(&self, new_data: Value) -> Result<(), String> {
        let json =
            serde_json::to_string_pretty(&new_data).map_err(|e| e.to_string())?;
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(&self.path, json).map_err(|e| e.to_string())?;
        if let Ok(mut g) = self.data.lock() {
            *g = new_data;
        }
        Ok(())
    }
}

pub fn default_config_path(config_dir: &Path) -> PathBuf {
    config_dir.join("app_config.json")
}
