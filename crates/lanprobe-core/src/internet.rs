use std::collections::VecDeque;
use std::net::{IpAddr, Ipv4Addr};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use ping_async::{IcmpEchoRequestor, IcmpEchoStatus};
use serde::Serialize;
use tokio::time::sleep;

use crate::interfaces::get_interface_details;

#[derive(Debug, Serialize, Clone, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum InternetState {
    Online,
    Limited,
    Offline,
}

pub fn derive_state(icmp_ok: bool, http_ok: bool) -> InternetState {
    // HTTP est le test principal : si la requête HTTPS passe → Online.
    // ICMP seul → Limited (ex : portail captif, pare-feu bloque HTTP).
    // Sur Linux desktop sans CAP_NET_RAW, ICMP échoue toujours ; sans
    // ce fallback l'état serait Offline alors que HTTP fonctionne.
    if http_ok {
        InternetState::Online
    } else if icmp_ok {
        InternetState::Limited
    } else {
        InternetState::Offline
    }
}

const UPTIME_WINDOW: usize = 720; // ~1h @ 5s tick
const ICMP_TARGET: &str = "1.1.1.1";
// Endpoint de détection de connexion (HTTP simple, 204 No Content, pas de TLS requis).
// Utilisé par Android/Chrome — fiable, lightweight, ne redirige pas.
const HTTP_TARGET: &str = "http://connectivitycheck.gstatic.com/generate_204";
const DNS_TARGET: &str = "cloudflare.com";
const TICK_SECS: u64 = 5;

#[derive(Debug, Serialize, Clone)]
pub struct InternetTick {
    pub state: InternetState,
    pub icmp_ok: bool,
    pub icmp_ms: Option<u64>,
    pub http_ok: bool,
    pub http_ms: Option<u64>,
    pub dns_ok: bool,
    pub dns_ms: Option<u64>,
    pub dns_target: &'static str,
    pub icmp_target: &'static str,
    pub http_target: &'static str,
    pub timestamp: u64,
    pub uptime_pct: f64,
    pub samples: usize,
}

#[derive(Default)]
pub struct InternetHistory {
    buf: Mutex<VecDeque<bool>>,
    last: Mutex<Option<InternetTick>>,
}

impl InternetHistory {
    pub fn push(&self, icmp_ok: bool) -> (f64, usize) {
        let Ok(mut buf) = self.buf.lock() else { return (100.0, 0); };
        if buf.len() >= UPTIME_WINDOW { buf.pop_front(); }
        buf.push_back(icmp_ok);
        let total = buf.len();
        let ok = buf.iter().filter(|b| **b).count();
        let pct = if total == 0 { 100.0 } else { (ok as f64 / total as f64) * 100.0 };
        (pct, total)
    }

    pub fn set_last(&self, tick: InternetTick) {
        if let Ok(mut guard) = self.last.lock() {
            *guard = Some(tick);
        }
    }

    pub fn snapshot(&self) -> Option<InternetTick> {
        self.last.lock().ok().and_then(|g| g.clone())
    }

    /// Vide la fenêtre d'uptime et le dernier tick — utilisé quand on change
    /// de profil réseau (static/DHCP) pour que l'historique de l'interface
    /// précédente ne contamine pas le pourcentage affiché après le switch.
    pub fn reset(&self) {
        if let Ok(mut buf) = self.buf.lock() { buf.clear(); }
        if let Ok(mut last) = self.last.lock() { *last = None; }
    }
}

pub async fn probe_dns() -> (bool, Option<u64>) {
    let start = std::time::Instant::now();
    // Résolution DNS via getaddrinfo (pas liée à une interface spécifique,
    // utilise le resolver système). On mesure le temps total de résolution.
    let result = tokio::time::timeout(
        Duration::from_secs(3),
        tokio::task::spawn_blocking(|| {
            use std::net::ToSocketAddrs;
            (DNS_TARGET, 80u16).to_socket_addrs().map(|_| ())
        }),
    ).await;
    match result {
        Ok(Ok(Ok(_))) => (true, Some(start.elapsed().as_millis() as u64)),
        _ => (false, None),
    }
}

pub async fn probe_icmp(src: Option<Ipv4Addr>) -> (bool, Option<u64>) {
    let Ok(addr) = ICMP_TARGET.parse::<IpAddr>() else { return (false, None); };
    let src_addr: Option<IpAddr> = src.map(IpAddr::V4);
    match IcmpEchoRequestor::new(addr, src_addr, None, Some(Duration::from_millis(2000))) {
        Ok(req) => match req.send().await {
            Ok(r) if r.status() == IcmpEchoStatus::Success => {
                (true, Some(r.round_trip_time().as_millis() as u64))
            }
            _ => (false, None),
        },
        // Raw socket unavailable (no CAP_NET_RAW on Linux) → use system ping binary.
        Err(_) => probe_icmp_subprocess(ICMP_TARGET, src).await,
    }
}

/// Fallback ICMP via the system `ping` binary (already has cap_net_raw/setuid).
/// Used on Linux when the process lacks CAP_NET_RAW.
async fn probe_icmp_subprocess(target: &str, src: Option<Ipv4Addr>) -> (bool, Option<u64>) {
    let mut args: Vec<String> = vec![
        "-c".into(), "1".into(),
        "-W".into(), "2".into(),
        "-q".into(),
    ];
    if let Some(ip) = src {
        args.push("-I".into());
        args.push(ip.to_string());
    }
    args.push(target.into());

    let result = tokio::time::timeout(
        Duration::from_millis(3000),
        tokio::process::Command::new("ping")
            .args(&args)
            .output(),
    ).await;

    match result {
        Ok(Ok(out)) if out.status.success() => {
            let text = String::from_utf8_lossy(&out.stdout);
            (true, parse_ping_rtt_ms(&text))
        }
        _ => (false, None),
    }
}

/// Parses the avg RTT from `ping -q` output:
/// `rtt min/avg/max/mdev = 1.234/2.345/3.456/0.100 ms`
fn parse_ping_rtt_ms(output: &str) -> Option<u64> {
    for line in output.lines() {
        if line.contains("rtt") || line.contains("round-trip") {
            if let Some(eq) = line.find('=') {
                let values: Vec<&str> = line[eq + 1..].trim().splitn(4, '/').collect();
                if values.len() >= 2 {
                    if let Ok(avg) = values[1].trim().parse::<f64>() {
                        return Some(avg.round() as u64);
                    }
                }
            }
        }
    }
    None
}

pub async fn probe_http(src: Option<Ipv4Addr>) -> (bool, Option<u64>) {
    let mut builder = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .user_agent("LanProbe-Internet-Probe");
    if let Some(s) = src {
        builder = builder.local_address(IpAddr::V4(s));
    }
    let client = match builder.build() {
        Ok(c) => c,
        Err(_) => return (false, None),
    };
    let start = std::time::Instant::now();
    match client.head(HTTP_TARGET).send().await {
        Ok(r) if r.status().is_success() => {
            (true, Some(start.elapsed().as_millis() as u64))
        }
        _ => (false, None),
    }
}

pub type InternetStateHandle = Arc<InternetHistory>;
pub type SelectedInterfaceHandle = Arc<Mutex<Option<String>>>;

/// Résout l'IP source. Retour :
/// - Ok(None) : aucune interface choisie (route par défaut)
/// - Ok(Some) : interface résolue
/// - Err : interface choisie mais pas d'IP → les probes doivent échouer
///   (offline) au lieu d'emprunter la route par défaut.
fn resolve_src_from_iface(iface: &SelectedInterfaceHandle) -> Result<Option<Ipv4Addr>, ()> {
    let Ok(guard) = iface.lock() else { return Ok(None); };
    let Some(name) = guard.clone() else { return Ok(None); };
    drop(guard);
    let details = get_interface_details(&name);
    let Some(ip_str) = details.ip else { return Err(()); };
    ip_str.parse::<Ipv4Addr>().map(Some).map_err(|_| ())
}

/// Boucle de monitoring internet, runtime-agnostique. Le caller choisit où
/// la faire tourner (tauri::async_runtime::spawn côté desktop, tokio::spawn
/// côté serveur headless) et fournit un callback `emit` qui publie chaque
/// tick (websocket, event Tauri, …).
pub async fn run_internet_monitor<F>(
    history: InternetStateHandle,
    iface: SelectedInterfaceHandle,
    mut emit: F,
) where
    F: FnMut(&InternetTick) + Send + 'static,
{
    loop {
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();

            // On relit la source à chaque tick : si l'utilisateur change
            // d'interface, le prochain probe sortira par la nouvelle.
            // Si l'interface sélectionnée n'a pas d'IPv4 (Wintun, tunnel
            // sans config), on reporte offline directement au lieu de
            // retomber sur la route par défaut — c'est exactement ce que
            // l'utilisateur veut tester : "est-ce que CETTE interface a
            // accès à internet".
            let ((icmp_ok, icmp_ms), (http_ok, http_ms), (dns_ok, dns_ms)) =
                match resolve_src_from_iface(&iface) {
                    Ok(src) => tokio::join!(probe_icmp(src), probe_http(src), probe_dns()),
                    Err(_) => ((false, None), (false, None), (false, None)),
                };
            let state = derive_state(icmp_ok, http_ok);
            // L'uptime reflète la connectivité réelle : Online si HTTP OU ICMP
            // passe. Sur Linux sans socket RAW (ex. Ubuntu sans CAP_NET_RAW),
            // l'ICMP échoue systématiquement mais HTTP fonctionne → utiliser
            // icmp_ok seul ferait chuter l'uptime même quand on est Online.
            let (uptime_pct, samples) = history.push(icmp_ok || http_ok);
            let tick = InternetTick {
                state,
                icmp_ok, icmp_ms,
                http_ok, http_ms,
                dns_ok, dns_ms,
                dns_target: DNS_TARGET,
                icmp_target: ICMP_TARGET,
                http_target: HTTP_TARGET,
                timestamp,
                uptime_pct,
                samples,
            };
            history.set_last(tick.clone());
            emit(&tick);
            sleep(Duration::from_secs(TICK_SECS)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn online_when_both_ok() {
        assert_eq!(derive_state(true, true), InternetState::Online);
    }

    #[test]
    fn online_when_http_only() {
        // Linux desktop sans CAP_NET_RAW : ICMP échoue mais HTTP passe.
        assert_eq!(derive_state(false, true), InternetState::Online);
    }

    #[test]
    fn limited_when_icmp_only() {
        // Portail captif : ICMP passe, HTTP bloqué.
        assert_eq!(derive_state(true, false), InternetState::Limited);
    }

    #[test]
    fn offline_when_both_down() {
        assert_eq!(derive_state(false, false), InternetState::Offline);
    }

    #[test]
    fn history_uptime_empty() {
        let h = InternetHistory::default();
        let (pct, n) = h.push(true);
        assert_eq!(pct, 100.0);
        assert_eq!(n, 1);
    }

    #[test]
    fn history_uptime_mixed() {
        let h = InternetHistory::default();
        h.push(true);
        h.push(false);
        let (pct, n) = h.push(true);
        assert!((pct - 66.666).abs() < 0.1);
        assert_eq!(n, 3);
    }
}
