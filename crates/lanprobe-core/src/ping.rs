use serde::Serialize;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::{Duration, Instant};
use tokio::net::TcpSocket;
use tokio::sync::mpsc;
use tokio::time::timeout;
use ping_async::{IcmpEchoRequestor, IcmpEchoStatus};
#[cfg(any(target_os = "windows", target_os = "macos"))]
use super::proc::async_cmd;

#[derive(Debug, Serialize, Clone, Default)]
pub struct PingResult {
    pub ip: String,
    pub alive: bool,
    pub latency_ms: Option<u64>,
    pub timestamp: u64,
}

const PROBE_PORTS: &[u16] = &[80, 443, 22, 445, 8080, 21, 23, 8443, 139, 9100, 631, 3389];

pub async fn ping_once(ip: &str, src: Option<Ipv4Addr>) -> PingResult {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    if let Some(lat) = icmp_ping(ip, src, 1000).await {
        return PingResult { ip: ip.to_string(), alive: true, latency_ms: Some(lat), timestamp };
    }

    #[cfg(target_os = "windows")]
    if let Some(lat) = windows_ping_exe(ip, src, 1000).await {
        return PingResult { ip: ip.to_string(), alive: true, latency_ms: Some(lat), timestamp };
    }

    #[cfg(target_os = "macos")]
    if let Some(lat) = macos_ping_exe(ip, src, 1000).await {
        return PingResult { ip: ip.to_string(), alive: true, latency_ms: Some(lat), timestamp };
    }

    let start = Instant::now();
    if tcp_probe(ip, src, 1000).await {
        PingResult { ip: ip.to_string(), alive: true, latency_ms: Some(start.elapsed().as_millis() as u64), timestamp }
    } else {
        PingResult { ip: ip.to_string(), alive: false, latency_ms: None, timestamp }
    }
}

/// Variante avec retries — utilisée pour le scan réseau où la perte
/// occasionnelle d'un paquet ICMP fait disparaître des hôtes pourtant joignables.
/// Seul le sondage ICMP est retenté (le moins cher) ; les fallbacks ne tournent
/// qu'une fois pour ne pas pénaliser les IPs réellement mortes.
pub async fn ping_once_fast_retry(ip: &str, src: Option<Ipv4Addr>, attempts: usize) -> Option<u64> {
    // Sur macOS, le binaire /sbin/ping (setuid root, raw socket) est beaucoup
    // plus fiable pour les sweeps que ping-async via SOCK_DGRAM. On le tente
    // en premier avant le crate, pour capturer un maximum d'hôtes.
    #[cfg(target_os = "macos")]
    if let Some(lat) = macos_ping_exe(ip, src, 600).await {
        return Some(lat);
    }

    for _ in 0..attempts.max(1) {
        if let Some(lat) = icmp_ping(ip, src, 600).await {
            return Some(lat);
        }
    }
    // Si l'utilisateur n'a PAS sélectionné d'interface on peut élargir la
    // recherche (ping.exe, tcp_probe, fallback no-src). Si au contraire il
    // en a choisi une, on reste strict : tout fallback sans src enverrait
    // le trafic par la route par défaut et ferait apparaître des hôtes qui
    // ne sont PAS joignables depuis l'interface choisie — ce qui induit
    // l'utilisateur en erreur (bug rapporté quand le Wi-Fi sélectionné est
    // désactivé mais la découverte continuait de trouver des hôtes via
    // l'Ethernet par défaut).
    if src.is_none() {
        #[cfg(target_os = "windows")]
        if let Some(lat) = windows_ping_exe(ip, None, 600).await {
            return Some(lat);
        }
        let start = Instant::now();
        if tcp_probe(ip, None, 600).await {
            return Some(start.elapsed().as_millis() as u64);
        }
        return None;
    }
    #[cfg(target_os = "windows")]
    if let Some(lat) = windows_ping_exe(ip, src, 600).await {
        return Some(lat);
    }
    let start = Instant::now();
    if tcp_probe(ip, src, 600).await {
        Some(start.elapsed().as_millis() as u64)
    } else {
        None
    }
}

/// Unprivileged ICMP ping via the `ping-async` crate.
/// `src` (Some) force l'adresse source — utile pour router le ping via une
/// interface spécifique au lieu de la route par défaut.
async fn icmp_ping(ip: &str, src: Option<Ipv4Addr>, timeout_ms: u64) -> Option<u64> {
    let addr: IpAddr = ip.parse().ok()?;
    let src_addr: Option<IpAddr> = src.map(IpAddr::V4);
    let requestor = IcmpEchoRequestor::new(
        addr,
        src_addr,
        None,
        Some(Duration::from_millis(timeout_ms)),
    ).ok()?;
    let reply = requestor.send().await.ok()?;
    match reply.status() {
        IcmpEchoStatus::Success => {
            let rtt = reply.round_trip_time();
            Some(rtt.as_millis() as u64)
        }
        _ => None,
    }
}

/// Parallel TCP probe on common ports — fallback when ICMP is blocked.
/// Si `src` est fourni, chaque socket est bind sur cette adresse avant le
/// connect → le paquet sort par l'interface qui possède cette IP.
async fn tcp_probe(ip: &str, src: Option<Ipv4Addr>, timeout_ms: u64) -> bool {
    // JoinSet pour pouvoir abort toutes les connexions TCP en cours dès
    // qu'on a une réponse (ou au timeout). Sans ça, sur un scan /24 avec
    // beaucoup d'IPs mortes, Windows accumule des milliers de SYN en vol
    // qui saturent la pile réseau et faisaient crasher l'app.
    let (tx, mut rx) = mpsc::channel::<()>(1);
    let mut set = tokio::task::JoinSet::new();
    for &port in PROBE_PORTS {
        let tx = tx.clone();
        let addr = format!("{}:{}", ip, port);
        let src = src;
        set.spawn(async move {
            if let Ok(target) = addr.parse::<SocketAddr>() {
                let connect = async {
                    let socket = TcpSocket::new_v4().ok()?;
                    if let Some(s) = src {
                        socket.bind(SocketAddr::new(IpAddr::V4(s), 0)).ok()?;
                    }
                    socket.connect(target).await.ok()
                };
                if timeout(Duration::from_millis(timeout_ms), connect)
                    .await
                    .ok()
                    .flatten()
                    .is_some()
                {
                    let _ = tx.send(()).await;
                }
            }
        });
    }
    drop(tx);
    let deadline = tokio::time::sleep(Duration::from_millis(timeout_ms + 50));
    tokio::pin!(deadline);
    let alive = tokio::select! {
        Some(_) = rx.recv() => true,
        _ = &mut deadline => false,
    };
    set.abort_all();
    alive
}

/// Fallback Windows : ping.exe natif (toujours présent, pas d'admin requis).
/// `-S <src>` force l'adresse source pour router via l'interface choisie.
#[cfg(target_os = "windows")]
async fn windows_ping_exe(ip: &str, src: Option<Ipv4Addr>, timeout_ms: u64) -> Option<u64> {
    let timeout_str = timeout_ms.to_string();
    let mut cmd = async_cmd("ping");
    cmd.args(["-n", "1", "-w", &timeout_str]);
    let src_str;
    if let Some(s) = src {
        src_str = s.to_string();
        cmd.args(["-S", &src_str]);
    }
    cmd.arg(ip);
    let out = cmd.output().await.ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    if !text.to_lowercase().contains("ttl=") {
        return None;
    }
    let lower = text.to_lowercase();
    for marker in ["time=", "tiempo=", "tempo=", "temps=", "zeit="] {
        if let Some(pos) = lower.find(marker) {
            let tail = &text[pos + marker.len()..];
            let digits: String = tail.chars()
                .skip_while(|c| !c.is_ascii_digit())
                .take_while(|c| c.is_ascii_digit())
                .collect();
            if let Ok(n) = digits.parse::<u64>() { return Some(n); }
        }
    }
    let bytes = lower.as_bytes();
    for i in 0..bytes.len().saturating_sub(2) {
        if &bytes[i..i + 2] == b"ms" {
            let mut j = i;
            while j > 0 && bytes[j - 1].is_ascii_digit() { j -= 1; }
            if j < i {
                if let Ok(n) = lower[j..i].parse::<u64>() { return Some(n); }
            }
        }
    }
    Some(0)
}

/// Fallback macOS : `/sbin/ping` (setuid root, raw socket, pas de password).
/// Beaucoup plus fiable que ping-async pour les sweeps réseau — le
/// `SOCK_DGRAM` ICMP de macOS limite le débit/fiabilité.
/// `-S <src>` force l'adresse source pour router via l'interface choisie.
#[cfg(target_os = "macos")]
async fn macos_ping_exe(ip: &str, src: Option<Ipv4Addr>, timeout_ms: u64) -> Option<u64> {
    // -c 1      : un seul echo request
    // -W <ms>   : timeout en millisecondes (macOS ping accepte des ms)
    // -n        : pas de résolution DNS
    // -q        : mode silencieux — on parse juste les stats de fin
    let timeout_str = timeout_ms.to_string();
    let mut cmd = async_cmd("/sbin/ping");
    cmd.args(["-c", "1", "-n", "-W", &timeout_str]);
    let src_str;
    if let Some(s) = src {
        src_str = s.to_string();
        cmd.args(["-S", &src_str]);
    }
    cmd.arg(ip);
    let out = cmd.output().await.ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    // Ligne type : "64 bytes from 192.168.1.1: icmp_seq=0 ttl=64 time=1.234 ms"
    for line in text.lines() {
        if let Some(pos) = line.find("time=") {
            let tail = &line[pos + 5..];
            let num: String = tail.chars()
                .take_while(|c| c.is_ascii_digit() || *c == '.')
                .collect();
            if let Ok(ms) = num.parse::<f64>() {
                return Some(ms.round() as u64);
            }
        }
    }
    None
}
