//! Export de métriques vers InfluxDB v1 ou v2.
//!
//! Le module écoute le bus `broadcast::Sender<BroadcastEvent>` et convertit
//! chaque event en lignes InfluxDB Line Protocol. Les lignes sont bufférisées
//! et flushées toutes les secondes en une seule requête POST.
//!
//! Config lue depuis `AppState::config` (clé `"influxdb"` dans
//! `app_config.json`). Le module attend silencieusement un `config:update`
//! valide avant de démarrer l'envoi.

use base64::{engine::general_purpose::STANDARD as B64_STANDARD, Engine};

use crate::state::{AppState, BroadcastEvent};

// ── Config structs ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct InfluxConfig {
    #[serde(default)]
    pub enabled: bool,
    /// "v1" or "v2"
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub url: String,
    /// Surcharge le hostname détecté automatiquement pour le tag `host=`.
    #[serde(default)]
    pub instance_label: String,
    #[serde(default)]
    pub v1: V1Config,
    #[serde(default)]
    pub v2: V2Config,
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct V1Config {
    #[serde(default)]
    pub database: String,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: String,
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct V2Config {
    #[serde(default)]
    pub org: String,
    #[serde(default)]
    pub bucket: String,
    #[serde(default)]
    pub token: String,
}

impl InfluxConfig {
    pub fn is_ready(&self) -> bool {
        if !self.enabled || self.url.is_empty() {
            return false;
        }
        match self.version.as_str() {
            "v1" => !self.v1.database.is_empty(),
            "v2" | "" => {
                !self.v2.org.is_empty()
                    && !self.v2.bucket.is_empty()
                    && !self.v2.token.is_empty()
            }
            _ => false,
        }
    }
}

// ── HTTP client ────────────────────────────────────────────────────────────

struct InfluxClient {
    http: reqwest::Client,
    base_url: String,
    query_params: Vec<(String, String)>,
    auth_header: Option<String>,
    host_tag: String,
}

impl InfluxClient {
    async fn new(cfg: &InfluxConfig) -> Self {
        let host_tag = resolve_host_tag_async(cfg).await;

        let (base_url, query_params, auth_header) = if cfg.version == "v1" {
            // Credentials en query params (InfluxDB v1 les supporte nativement).
            // On préfère ça à Basic Auth pour éviter une dépendance optionnelle
            // tout en restant correct — mais la crate base64 étant disponible,
            // on utilise quand même Basic Auth pour plus de sécurité sur HTTPS.
            let url = format!("{}/write", cfg.url.trim_end_matches('/'));
            let params = vec![
                ("db".to_string(), cfg.v1.database.clone()),
                ("precision".to_string(), "ns".to_string()),
            ];
            let auth = if !cfg.v1.username.is_empty() {
                let encoded =
                    B64_STANDARD.encode(format!("{}:{}", cfg.v1.username, cfg.v1.password));
                Some(format!("Basic {}", encoded))
            } else {
                None
            };
            (url, params, auth)
        } else {
            let url = format!("{}/api/v2/write", cfg.url.trim_end_matches('/'));
            let params = vec![
                ("org".to_string(), cfg.v2.org.clone()),
                ("bucket".to_string(), cfg.v2.bucket.clone()),
                ("precision".to_string(), "ns".to_string()),
            ];
            let auth = if !cfg.v2.token.is_empty() {
                Some(format!("Token {}", cfg.v2.token))
            } else {
                None
            };
            (url, params, auth)
        };

        Self {
            http: reqwest::Client::new(),
            base_url,
            query_params,
            auth_header,
            host_tag,
        }
    }

    async fn write(&self, body: String) -> Result<(), String> {
        let mut req = self.http
            .post(&self.base_url)
            .query(&self.query_params)
            .timeout(std::time::Duration::from_secs(10))
            .body(body);
        if let Some(auth) = &self.auth_header {
            req = req.header("Authorization", auth);
        }
        let resp = req.send().await.map_err(|e| e.to_string())?;
        let status = resp.status();
        if status.is_success() {
            Ok(())
        } else {
            Err(format!("InfluxDB write error: {}", status))
        }
    }
}

// ── Helpers ────────────────────────────────────────────────────────────────

async fn resolve_host_tag_async(cfg: &InfluxConfig) -> String {
    if !cfg.instance_label.is_empty() {
        return cfg.instance_label.clone();
    }
    if let Ok(h) = std::env::var("HOSTNAME") {
        if !h.is_empty() {
            return h;
        }
    }
    tokio::process::Command::new("hostname")
        .output()
        .await
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

/// Escapes spaces, commas and equals signs as required by InfluxDB Line Protocol.
fn escape_tag(s: &str) -> String {
    s.replace(',', "\\,").replace('=', "\\=").replace(' ', "\\ ")
}

/// Escapes backslashes and double-quotes for InfluxDB Line Protocol string field values.
fn escape_field_str(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Converts a `BroadcastEvent` to zero or more InfluxDB line protocol strings.
fn event_to_points(event: &BroadcastEvent, host: &str) -> Vec<String> {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let host_tag = escape_tag(host);
    let p = &event.payload;

    match event.event.as_str() {
        "ping:tick" => {
            let ip = p["ip"].as_str().unwrap_or("unknown");
            let alive = p["alive"].as_bool().unwrap_or(false);
            let alive_str = if alive { "true" } else { "false" };
            // latency_ms n'est inclus que quand l'hôte est joignable — écrire
            // 0 pour un hôte unreachable serait trompeur.
            let latency_part = if alive {
                format!("latency_ms={}i,", p["latency_ms"].as_u64().unwrap_or(0))
            } else {
                String::new()
            };
            vec![format!(
                "ping_latency,host={},ip={} {}alive={} {}",
                host_tag,
                escape_tag(ip),
                latency_part,
                alive_str,
                ts
            )]
        }

        "internet:tick" => {
            // `state` est sérialisé comme une string (snake_case via serde).
            let state = p["state"].as_str().unwrap_or("unknown");
            let icmp = p["icmp_ms"].as_u64().unwrap_or(0);
            let http = p["http_ms"].as_u64().unwrap_or(0);
            let dns = p["dns_ms"].as_u64().unwrap_or(0);
            let uptime = p["uptime_pct"].as_f64().unwrap_or(0.0);
            let icmp_ok = if p["icmp_ok"].as_bool().unwrap_or(false) {
                "true"
            } else {
                "false"
            };
            let http_ok = if p["http_ok"].as_bool().unwrap_or(false) {
                "true"
            } else {
                "false"
            };
            let dns_ok = if p["dns_ok"].as_bool().unwrap_or(false) {
                "true"
            } else {
                "false"
            };
            vec![format!(
                "internet_status,host={} state=\"{}\",icmp_ms={}i,http_ms={}i,dns_ms={}i,uptime_pct={},icmp_ok={},http_ok={},dns_ok={} {}",
                host_tag,
                escape_field_str(state),
                icmp,
                http,
                dns,
                uptime,
                icmp_ok,
                http_ok,
                dns_ok,
                ts
            )]
        }

        "speedtest:result" => {
            let engine = p["engine"].as_str().unwrap_or("unknown");
            let dl = p["download_mbps"].as_f64().unwrap_or(0.0);
            let ul = p["upload_mbps"].as_f64().unwrap_or(0.0);
            let lat = p["latency_ms"].as_u64().unwrap_or(0);
            // jitter_ms est Option<f64> dans SpeedResult.
            let jitter = p["jitter_ms"].as_f64().unwrap_or(0.0);
            vec![format!(
                "speedtest,host={},engine={} download_mbps={},upload_mbps={},latency_ms={}i,jitter_ms={} {}",
                host_tag,
                escape_tag(engine),
                dl,
                ul,
                lat,
                jitter,
                ts
            )]
        }

        "discovery:done" => {
            let cidr = p["cidr"].as_str().unwrap_or("");
            let hosts = p["hosts_found"].as_i64().unwrap_or(0);
            if cidr.is_empty() {
                return vec![];
            }
            vec![format!(
                "discovery,host={},cidr={} hosts_found={}i {}",
                host_tag,
                escape_tag(cidr),
                hosts,
                ts
            )]
        }

        "portscan:update" => {
            let ip = p["ip"].as_str().unwrap_or("unknown");
            // Le payload est un PortScanEntry sérialisé :
            // { ip, tcp: [...], udp: [...], timestamp, profile_id, in_progress }
            // On ne log pas les scans en cours — attendre la fin.
            if p["in_progress"].as_bool().unwrap_or(false) {
                return vec![];
            }
            let open_tcp = p["tcp"]
                .as_array()
                .map(|a| a.len() as i64)
                .unwrap_or(0);
            let open_udp = p["udp"]
                .as_array()
                .map(|a| a.len() as i64)
                .unwrap_or(0);
            vec![format!(
                "port_scan,host={},ip={} open_tcp={}i,open_udp={}i {}",
                host_tag,
                escape_tag(ip),
                open_tcp,
                open_udp,
                ts
            )]
        }

        _ => vec![],
    }
}

// ── Config loader ──────────────────────────────────────────────────────────

fn load_config(state: &AppState) -> InfluxConfig {
    let cfg_value = state.config.get();
    cfg_value
        .get("influxdb")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default()
}

// ── Public API ─────────────────────────────────────────────────────────────

/// Teste la connectivité vers l'endpoint InfluxDB configuré.
/// Retourne `Ok(())` si le serveur répond avec 200 ou 204, une erreur
/// descriptive sinon.
pub async fn test_connection(state: AppState) -> Result<(), String> {
    let cfg = load_config(&state);
    if !cfg.enabled {
        return Err("InfluxDB is not enabled".to_string());
    }
    if cfg.url.is_empty() {
        return Err("InfluxDB URL is not configured".to_string());
    }
    let ping_url = if cfg.version == "v1" {
        format!("{}/ping", cfg.url.trim_end_matches('/'))
    } else {
        format!("{}/api/v2/ping", cfg.url.trim_end_matches('/'))
    };
    let client = InfluxClient::new(&cfg).await;
    let mut req = client.http.get(&ping_url);
    if let Some(auth) = &client.auth_header {
        req = req.header("Authorization", auth);
    }
    let resp = req
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let status = resp.status();
    if status.is_success() {
        Ok(())
    } else {
        Err(format!("Unexpected status: {}", status))
    }
}

/// Tâche de fond — souscrit aux events et pousse les métriques vers
/// InfluxDB. Bloque jusqu'à la fermeture du canal broadcast.
///
/// Le flux de contrôle :
/// 1. On souscrit AVANT de lire la config (évite de rater des events).
/// 2. Si la config n'est pas prête, on attend un `config:update` valide.
/// 3. On tourne ensuite dans une boucle select : on bufférise les points et
///    on flushe toutes les secondes.
/// 4. Sur `config:update`, on recharge. Si InfluxDB est désactivé, on vide
///    le buffer et on attend la réactivation.
pub async fn run(state: AppState) {
    // 1. S'abonner avant de lire la config.
    let mut rx = state.events.subscribe();

    // 2. Charger la config initiale.
    let mut cfg = load_config(&state);

    // 3. Attendre une config valide.
    while !cfg.is_ready() {
        match rx.recv().await {
            Ok(event) if event.event == "config:update" => {
                cfg = load_config(&state);
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                tracing::warn!("InfluxDB: broadcast lagged, {} events dropped", n);
            }
            _ => {}
        }
    }

    // 4. Construire le client.
    let mut client = InfluxClient::new(&cfg).await;

    // 5. Ticker de flush à 1 s.
    let mut ticker = tokio::time::interval(std::time::Duration::from_secs(1));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    let mut buffer: Vec<String> = Vec::new();

    loop {
        tokio::select! {
            event_result = rx.recv() => {
                match event_result {
                    Ok(event) => {
                        if event.event == "config:update" {
                            let new_cfg = load_config(&state);
                            if new_cfg.is_ready() {
                                client = InfluxClient::new(&new_cfg).await;
                                cfg = new_cfg;
                            } else {
                                // InfluxDB désactivé ou config incomplète →
                                // vider le buffer et attendre la réactivation.
                                buffer.clear();
                                cfg = new_cfg;
                                while !cfg.is_ready() {
                                    match rx.recv().await {
                                        Ok(e) if e.event == "config:update" => {
                                            cfg = load_config(&state);
                                        }
                                        Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
                                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                                            tracing::warn!("InfluxDB: broadcast lagged, {} events dropped", n);
                                        }
                                        _ => {}
                                    }
                                }
                                client = InfluxClient::new(&cfg).await;
                            }
                        } else {
                            let points = event_to_points(&event, &client.host_tag);
                            buffer.extend(points);
                            const MAX_BUFFER: usize = 10_000;
                            if buffer.len() > MAX_BUFFER {
                                let drop_count = buffer.len() - MAX_BUFFER;
                                buffer.drain(0..drop_count);
                                tracing::warn!("InfluxDB: buffer overflow, dropped {} oldest points", drop_count);
                            }
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!("InfluxDB: broadcast lagged, {} events dropped", n);
                    }
                }
            }
            _ = ticker.tick() => {
                if !buffer.is_empty() {
                    let body = buffer.join("\n");
                    buffer.clear();
                    if let Err(e) = client.write(body).await {
                        tracing::warn!("InfluxDB write failed: {}", e);
                    }
                }
            }
        }
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_event(event: &str, payload: serde_json::Value) -> BroadcastEvent {
        BroadcastEvent {
            event: event.to_string(),
            payload,
        }
    }

    #[test]
    fn test_influx_config_default() {
        assert!(!InfluxConfig::default().is_ready());
    }

    #[test]
    fn test_influx_config_v1_ready() {
        let cfg = InfluxConfig {
            enabled: true,
            version: "v1".to_string(),
            url: "http://localhost:8086".to_string(),
            instance_label: String::new(),
            v1: V1Config {
                database: "mydb".to_string(),
                username: String::new(),
                password: String::new(),
            },
            v2: V2Config::default(),
        };
        assert!(cfg.is_ready());
    }

    #[test]
    fn test_influx_config_v2_ready() {
        let cfg = InfluxConfig {
            enabled: true,
            version: "v2".to_string(),
            url: "http://localhost:8086".to_string(),
            instance_label: String::new(),
            v1: V1Config::default(),
            v2: V2Config {
                org: "myorg".to_string(),
                bucket: "mybucket".to_string(),
                token: "mytoken".to_string(),
            },
        };
        assert!(cfg.is_ready());
    }

    #[test]
    fn test_escape_tag() {
        assert_eq!(
            escape_tag("hello world,foo=bar"),
            "hello\\ world\\,foo\\=bar"
        );
    }

    #[test]
    fn test_escape_field_str() {
        assert_eq!(escape_field_str("hello"), "hello");
        assert_eq!(escape_field_str("say \"hi\""), "say \\\"hi\\\"");
        assert_eq!(escape_field_str("path\\to"), "path\\\\to");
    }

    #[test]
    fn test_event_to_points_ping() {
        let event = make_event(
            "ping:tick",
            json!({ "ip": "192.168.1.1", "alive": true, "latency_ms": 5 }),
        );
        let points = event_to_points(&event, "testhost");
        assert_eq!(points.len(), 1);
        assert!(points[0].starts_with("ping_latency,host=testhost,ip=192.168.1.1 "));
    }

    #[test]
    fn test_event_to_points_internet() {
        let event = make_event(
            "internet:tick",
            json!({
                "state": "up",
                "icmp_ms": 5,
                "http_ms": 10,
                "dns_ms": 8,
                "uptime_pct": 99.9,
                "icmp_ok": true,
                "http_ok": true,
                "dns_ok": true
            }),
        );
        let points = event_to_points(&event, "testhost");
        assert_eq!(points.len(), 1);
        assert!(points[0].starts_with("internet_status,host=testhost "));
    }

    #[test]
    fn test_event_to_points_speedtest() {
        let event = make_event(
            "speedtest:result",
            json!({
                "engine": "ookla",
                "download_mbps": 100.5,
                "upload_mbps": 50.2,
                "latency_ms": 12,
                "jitter_ms": 1.5,
                "server_name": "Paris, FR"
            }),
        );
        let points = event_to_points(&event, "testhost");
        assert_eq!(points.len(), 1);
        assert!(points[0].contains(",engine=ookla "), "Expected ',engine=ookla ' in: {}", points[0]);
    }

    #[test]
    fn test_event_to_points_discovery() {
        let event = make_event(
            "discovery:done",
            json!({ "cidr": "10.0.0.0/24", "hosts_found": 3 }),
        );
        let points = event_to_points(&event, "testhost");
        assert_eq!(points.len(), 1);
        assert!(points[0].contains("cidr=10.0.0.0/24"), "Expected cidr tag in: {}", points[0]);
        assert!(points[0].contains("hosts_found=3i"), "Expected hosts_found in: {}", points[0]);
    }

    #[test]
    fn test_event_to_points_discovery_empty_cidr() {
        let event = BroadcastEvent {
            event: "discovery:done".to_string(),
            payload: serde_json::json!({ "cidr": "", "hosts_found": 0 }),
        };
        let points = event_to_points(&event, "testhost");
        assert!(points.is_empty(), "empty cidr should produce no points");
    }

    #[test]
    fn test_event_to_points_portscan_in_progress() {
        let event = BroadcastEvent {
            event: "portscan:update".to_string(),
            payload: serde_json::json!({ "ip": "192.168.1.1", "tcp": [], "udp": [], "in_progress": true }),
        };
        let points = event_to_points(&event, "testhost");
        assert!(points.is_empty(), "in-progress scan should produce no points");
    }

    #[test]
    fn test_event_to_points_portscan() {
        let event = make_event(
            "portscan:update",
            json!({
                "ip": "192.168.1.100",
                "tcp": [
                    { "port": 22, "service": "SSH", "proto": "tcp", "open": true },
                    { "port": 80, "service": "HTTP", "proto": "tcp", "open": true }
                ],
                "udp": [],
                "timestamp": 1700000000u64,
                "profile_id": null,
                "in_progress": false
            }),
        );
        let points = event_to_points(&event, "testhost");
        assert_eq!(points.len(), 1);
        assert!(points[0].contains("open_tcp=2i"), "Expected open_tcp=2i in: {}", points[0]);
        assert!(points[0].contains("open_udp=0i"), "Expected open_udp=0i in: {}", points[0]);
    }

    #[test]
    fn test_event_to_points_unknown() {
        let event = make_event("unknown:event", json!({}));
        let points = event_to_points(&event, "testhost");
        assert!(points.is_empty());
    }
}
