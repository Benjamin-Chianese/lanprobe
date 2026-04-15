//! Auth minimaliste stockée dans `users.json` à côté de la config du serveur.
//!
//! - Passwords hashés en argon2id (jamais en clair)
//! - Sessions en mémoire : token 32 bytes aléatoires dans un cookie HttpOnly
//! - Premier lancement : si users.json n'existe pas, le serveur expose une
//!   route `/api/setup` qui crée le premier admin (login + password) ; toutes
//!   les autres routes renvoient 503 avec `{ needs_setup: true }` jusque là.
//!
//! Pas de BDD — tant qu'on reste sur un admin + quelques users c'est suffisant.
//! Si un jour il faut de l'audit log ou du multi-tenant on migrera vers SQLite.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use serde::{Deserialize, Serialize};

const SESSION_TTL: Duration = Duration::from_secs(60 * 60 * 24 * 7);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserRecord {
    pub username: String,
    pub password_hash: String,
    pub role: String,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
struct UsersFile {
    users: Vec<UserRecord>,
}

pub struct AuthStore {
    path: PathBuf,
    users: Mutex<Vec<UserRecord>>,
    sessions: Mutex<HashMap<String, Session>>,
}

struct Session {
    username: String,
    expires: Instant,
}

impl AuthStore {
    pub fn load(path: PathBuf) -> std::io::Result<Self> {
        let users = if path.exists() {
            let raw = std::fs::read_to_string(&path)?;
            let parsed: UsersFile = serde_json::from_str(&raw).unwrap_or_default();
            parsed.users
        } else {
            Vec::new()
        };
        Ok(Self {
            path,
            users: Mutex::new(users),
            sessions: Mutex::new(HashMap::new()),
        })
    }

    pub fn needs_setup(&self) -> bool {
        self.users.lock().map(|u| u.is_empty()).unwrap_or(true)
    }

    /// Crée le premier utilisateur admin. Échoue si un user existe déjà —
    /// la route `/api/setup` n'est pas rejouable.
    pub fn initial_setup(&self, username: &str, password: &str) -> Result<(), String> {
        let mut guard = self.users.lock().map_err(|_| "users lock poisoned".to_string())?;
        if !guard.is_empty() {
            return Err("setup already done".into());
        }
        let hash = hash_password(password).map_err(|e| e.to_string())?;
        guard.push(UserRecord {
            username: username.to_string(),
            password_hash: hash,
            role: "admin".into(),
        });
        self.persist(&guard)?;
        Ok(())
    }

    /// Crée ou met à jour les identifiants de l'administrateur.
    /// Contrairement à `initial_setup`, fonctionne même si un compte existe déjà.
    pub fn set_or_update_credentials(&self, username: &str, password: &str) -> Result<(), String> {
        let mut guard = self.users.lock().map_err(|_| "users lock poisoned".to_string())?;
        let hash = hash_password(password).map_err(|e| e.to_string())?;
        if let Some(user) = guard.iter_mut().find(|u| u.role == "admin") {
            user.username = username.to_string();
            user.password_hash = hash;
        } else {
            guard.push(UserRecord {
                username: username.to_string(),
                password_hash: hash,
                role: "admin".into(),
            });
        }
        self.persist(&guard)
    }

    pub fn login(&self, username: &str, password: &str) -> Result<String, String> {
        let guard = self.users.lock().map_err(|_| "users lock poisoned".to_string())?;
        let user = guard
            .iter()
            .find(|u| u.username == username)
            .ok_or_else(|| "invalid credentials".to_string())?;
        verify_password(&user.password_hash, password).map_err(|_| "invalid credentials".to_string())?;
        let token = generate_token();
        let mut sessions = self.sessions.lock().map_err(|_| "sessions lock poisoned".to_string())?;
        sessions.insert(
            token.clone(),
            Session {
                username: user.username.clone(),
                expires: Instant::now() + SESSION_TTL,
            },
        );
        Ok(token)
    }

    pub fn validate(&self, token: &str) -> Option<String> {
        let mut sessions = self.sessions.lock().ok()?;
        let session = sessions.get(token)?;
        if session.expires < Instant::now() {
            sessions.remove(token);
            return None;
        }
        Some(session.username.clone())
    }

    pub fn logout(&self, token: &str) {
        if let Ok(mut sessions) = self.sessions.lock() {
            sessions.remove(token);
        }
    }

    fn persist(&self, users: &[UserRecord]) -> Result<(), String> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let data = UsersFile { users: users.to_vec() };
        let json = serde_json::to_string_pretty(&data).map_err(|e| e.to_string())?;
        std::fs::write(&self.path, json).map_err(|e| e.to_string())?;
        // Restreint à lecture user-only — évite d'exposer les hashes au
        // reste du système dans un home partagé.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&self.path, std::fs::Permissions::from_mode(0o600));
        }
        Ok(())
    }
}

fn hash_password(password: &str) -> Result<String, argon2::password_hash::Error> {
    // 16 bytes de salt brut → base64 via SaltString (argon2 0.5 ne prend pas
    // directement rand 0.10 en entrée, on contourne avec getrandom).
    let mut salt_bytes = [0u8; 16];
    getrandom::getrandom(&mut salt_bytes).map_err(|_| argon2::password_hash::Error::Crypto)?;
    let salt = SaltString::encode_b64(&salt_bytes)?;
    let argon = Argon2::default();
    let hash = argon.hash_password(password.as_bytes(), &salt)?;
    Ok(hash.to_string())
}

fn verify_password(stored: &str, password: &str) -> Result<(), argon2::password_hash::Error> {
    let parsed = PasswordHash::new(stored)?;
    Argon2::default().verify_password(password.as_bytes(), &parsed)
}

fn generate_token() -> String {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    let mut bytes = [0u8; 32];
    getrandom::getrandom(&mut bytes).expect("getrandom failed");
    URL_SAFE_NO_PAD.encode(bytes)
}

pub fn default_users_path(config_dir: &Path) -> PathBuf {
    config_dir.join("users.json")
}
