//! Génération/chargement d'un certificat self-signed pour le mode serveur.
//!
//! Au premier lancement on génère un cert ECDSA P-256 valide 10 ans (CN =
//! hostname de la machine) et on le persiste à côté de users.json. Les
//! navigateurs afficheront un warning "not trusted" — à l'utilisateur
//! d'accepter l'exception une fois par poste, ou de mettre LanProbe derrière
//! un reverse-proxy type Caddy pour un vrai cert.
//!
//! On ne veut surtout PAS que le password transite en HTTP clair, même en
//! LAN : d'où le cert auto-généré plutôt que du HTTP tout court.

use std::path::{Path, PathBuf};
use std::sync::Arc;

pub struct TlsPaths {
    pub cert: PathBuf,
    pub key: PathBuf,
}

pub fn tls_paths(config_dir: &Path) -> TlsPaths {
    TlsPaths {
        cert: config_dir.join("server.crt"),
        key: config_dir.join("server.key"),
    }
}

/// Charge la config rustls depuis les fichiers PEM pour `tokio_rustls::TlsAcceptor`.
pub async fn ensure_rustls_config(paths: &TlsPaths) -> Result<Arc<rustls::ServerConfig>, String> {
    if !paths.cert.exists() || !paths.key.exists() {
        generate_self_signed(paths)?;
    }
    let cert_pem = tokio::fs::read(&paths.cert).await.map_err(|e| e.to_string())?;
    let key_pem = tokio::fs::read(&paths.key).await.map_err(|e| e.to_string())?;

    let certs: Vec<rustls::pki_types::CertificateDer<'static>> =
        rustls_pemfile::certs(&mut cert_pem.as_slice())
            .filter_map(|r| r.ok())
            .map(|c| c.into_owned())
            .collect();
    let key = rustls_pemfile::private_key(&mut key_pem.as_slice())
        .map_err(|e| e.to_string())?
        .ok_or("no private key found")?
        .clone_key();

    let config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|e| e.to_string())?;

    Ok(Arc::new(config))
}

fn generate_self_signed(paths: &TlsPaths) -> Result<(), String> {
    use rcgen::{CertificateParams, DistinguishedName, DnType, KeyPair};

    let hostname = hostname().unwrap_or_else(|| "lanprobe.local".into());
    let mut params = CertificateParams::new(vec![
        hostname.clone(),
        "localhost".into(),
        "127.0.0.1".into(),
    ])
    .map_err(|e| e.to_string())?;
    let mut dn = DistinguishedName::new();
    dn.push(DnType::CommonName, hostname);
    dn.push(DnType::OrganizationName, "LanProbe");
    params.distinguished_name = dn;

    let key_pair = KeyPair::generate().map_err(|e| e.to_string())?;
    let cert = params.self_signed(&key_pair).map_err(|e| e.to_string())?;

    if let Some(parent) = paths.cert.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(&paths.cert, cert.pem()).map_err(|e| e.to_string())?;
    std::fs::write(&paths.key, key_pair.serialize_pem()).map_err(|e| e.to_string())?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&paths.key, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

fn hostname() -> Option<String> {
    std::process::Command::new("hostname")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}
