use serde::Serialize;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;
use tokio::net::{TcpSocket, UdpSocket};
use tokio::time::timeout;

#[derive(Debug, Serialize, Clone, Default)]
pub struct PortResult {
    pub port: u16,
    pub service: String,
    pub proto: String, // "tcp" | "udp"
    pub open: bool,
}

const COMMON_TCP_PORTS: &[(u16, &str)] = &[
    (21, "FTP"), (22, "SSH"), (23, "Telnet"), (25, "SMTP"), (53, "DNS"),
    (80, "HTTP"), (110, "POP3"), (143, "IMAP"), (443, "HTTPS"),
    (445, "SMB"), (3306, "MySQL"), (3389, "RDP"), (5432, "PostgreSQL"),
    (5900, "VNC"), (8080, "HTTP-Alt"), (8443, "HTTPS-Alt"),
];

/// UDP ports on lequel on sait envoyer une requête protocol-aware et détecter
/// une réponse. Sans raw sockets on ne peut pas voir les "ICMP port unreachable",
/// donc on se limite aux services qui répondent au niveau applicatif.
const COMMON_UDP_PORTS: &[(u16, &str)] = &[
    (53, "DNS"),
    (123, "NTP"),
    (137, "NetBIOS"),
    (161, "SNMP"),
    (1900, "SSDP"),
    (5353, "mDNS"),
];

/// Résout un nom de service pour un port TCP arbitraire. Si absent de
/// COMMON_TCP_PORTS, on retombe sur une petite table étendue puis sur
/// "Custom" — on veut juste un libellé affichable, pas une base IANA.
fn tcp_service_name(port: u16) -> &'static str {
    for (p, s) in COMMON_TCP_PORTS { if *p == port { return s; } }
    match port {
        9  => "Discard", 37 => "Time", 79 => "Finger", 113 => "Ident",
        119 => "NNTP", 135 => "RPC", 139 => "NetBIOS", 161 => "SNMP",
        389 => "LDAP", 465 => "SMTPS", 514 => "Syslog", 587 => "SMTP-Sub",
        631 => "IPP", 636 => "LDAPS", 873 => "Rsync", 993 => "IMAPS",
        995 => "POP3S", 1080 => "SOCKS", 1194 => "OpenVPN", 1433 => "MSSQL",
        1521 => "Oracle", 2049 => "NFS", 2082 => "cPanel", 2083 => "cPanelSSL",
        2222 => "SSH-Alt", 2375 => "Docker", 2376 => "DockerTLS",
        3000 => "Node/Grafana", 3128 => "Squid", 4444 => "Metasploit",
        5000 => "UPnP", 5060 => "SIP", 5222 => "XMPP", 5672 => "AMQP",
        5984 => "CouchDB", 6379 => "Redis", 6443 => "K8s-API",
        7000 => "Cassandra", 8000 => "HTTP-Alt", 8008 => "HTTP-Alt",
        8086 => "InfluxDB", 8088 => "HTTP-Alt", 8181 => "HTTP-Alt",
        8883 => "MQTTS", 9000 => "SonarQube", 9092 => "Kafka",
        9200 => "Elasticsearch", 9300 => "Elasticsearch", 11211 => "Memcached",
        27017 => "MongoDB", 50000 => "SAP",
        _ => "Custom",
    }
}

fn udp_service_name(port: u16) -> &'static str {
    for (p, s) in COMMON_UDP_PORTS { if *p == port { return s; } }
    match port {
        67 => "DHCP", 68 => "DHCP-Client", 69 => "TFTP", 111 => "Portmap",
        161 => "SNMP", 162 => "SNMP-Trap", 500 => "IKE", 514 => "Syslog",
        520 => "RIP", 1434 => "MSSQL-Browser", 4500 => "IPsec-NAT",
        _ => "Custom",
    }
}

pub async fn scan_ports(ip: &str, src: Option<Ipv4Addr>, ports: Option<Vec<u16>>) -> Vec<PortResult> {
    // Liste par défaut = COMMON_TCP_PORTS, sinon la liste custom fournie
    // par le profil utilisateur (résolue côté frontend).
    let port_list: Vec<(u16, String)> = match ports {
        Some(list) => list.into_iter()
            .map(|p| (p, tcp_service_name(p).to_string()))
            .collect(),
        None => COMMON_TCP_PORTS.iter()
            .map(|(p, s)| (*p, s.to_string()))
            .collect(),
    };

    let target_ip: IpAddr = match ip.parse() {
        Ok(a) => a,
        Err(_) => return port_list.into_iter()
            .map(|(p, s)| PortResult { port: p, service: s, proto: "tcp".into(), open: false })
            .collect(),
    };

    let mut handles = vec![];
    for (port, service) in port_list {
        let target = SocketAddr::new(target_ip, port);
        let src = src;
        handles.push(tokio::spawn(async move {
            let connect = async {
                let socket = TcpSocket::new_v4().ok()?;
                if let Some(s) = src {
                    socket.bind(SocketAddr::new(IpAddr::V4(s), 0)).ok()?;
                }
                socket.connect(target).await.ok()
            };
            let open = timeout(Duration::from_millis(1000), connect)
                .await
                .ok()
                .flatten()
                .is_some();
            PortResult { port, service, proto: "tcp".into(), open }
        }));
    }

    let mut results: Vec<PortResult> = Vec::with_capacity(handles.len());
    for h in handles {
        if let Ok(r) = h.await { results.push(r); }
    }
    results.sort_by_key(|r| r.port);
    results
}

pub async fn scan_udp_ports(ip: &str, src: Option<Ipv4Addr>, ports: Option<Vec<u16>>) -> Vec<PortResult> {
    let port_list: Vec<(u16, String)> = match ports {
        Some(list) => list.into_iter()
            .map(|p| (p, udp_service_name(p).to_string()))
            .collect(),
        None => COMMON_UDP_PORTS.iter()
            .map(|(p, s)| (*p, s.to_string()))
            .collect(),
    };

    let target_ip: IpAddr = match ip.parse() {
        Ok(a) => a,
        Err(_) => return port_list.into_iter()
            .map(|(p, s)| PortResult { port: p, service: s, proto: "udp".into(), open: false })
            .collect(),
    };
    let target_v4 = match target_ip {
        IpAddr::V4(v4) => v4,
        _ => return vec![],
    };

    let mut handles = vec![];
    for (port, service) in port_list {
        let src = src;
        handles.push(tokio::spawn(async move {
            let open = udp_probe(target_v4, port, src, 1200).await;
            PortResult { port, service, proto: "udp".into(), open }
        }));
    }

    let mut results: Vec<PortResult> = Vec::with_capacity(handles.len());
    for h in handles {
        if let Ok(r) = h.await { results.push(r); }
    }
    results.sort_by_key(|r| r.port);
    results
}

/// Envoie un datagramme protocol-aware et considère le port ouvert si le
/// service répond. Silence = filtered/closed/unknown → open=false.
async fn udp_probe(target: Ipv4Addr, port: u16, src: Option<Ipv4Addr>, timeout_ms: u64) -> bool {
    let bind_addr = match src {
        Some(s) => SocketAddr::new(IpAddr::V4(s), 0),
        None => "0.0.0.0:0".parse().unwrap(),
    };
    let Ok(sock) = UdpSocket::bind(bind_addr).await else { return false };
    let target_sa = SocketAddr::new(IpAddr::V4(target), port);

    let probe: Vec<u8> = match port {
        53 => dns_query(),
        123 => ntp_request(),
        137 => netbios_name_query(),
        161 => snmp_get_public(),
        1900 => ssdp_msearch(),
        5353 => mdns_query(),
        _ => vec![0u8; 8],
    };
    if sock.send_to(&probe, target_sa).await.is_err() { return false; }

    let mut buf = [0u8; 1500];
    match timeout(Duration::from_millis(timeout_ms), sock.recv_from(&mut buf)).await {
        Ok(Ok((n, from))) if n > 0 && from.ip() == IpAddr::V4(target) => true,
        _ => false,
    }
}

fn dns_query() -> Vec<u8> {
    // DNS query for "." type NS (root) — courte, répondue par n'importe quel
    // résolveur récursif ou autoritaire ; suffit pour détecter un DNS actif.
    let mut q = vec![
        0xaa, 0xaa, // id
        0x01, 0x00, // flags: standard query, recursion desired
        0x00, 0x01, // qdcount
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00,       // root label
        0x00, 0x02, // qtype = NS
        0x00, 0x01, // qclass = IN
    ];
    q.shrink_to_fit();
    q
}

fn ntp_request() -> Vec<u8> {
    let mut buf = vec![0u8; 48];
    // LI=0, VN=4, Mode=3 (client)
    buf[0] = 0x23;
    buf
}

fn netbios_name_query() -> Vec<u8> {
    // NBSTAT query "*" (wildcard) — révèle les services NetBIOS d'une machine Windows.
    vec![
        0xaa, 0xaa, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x20, b'C', b'K', b'A', b'A', b'A', b'A', b'A', b'A', b'A', b'A', b'A',
        b'A', b'A', b'A', b'A', b'A', b'A', b'A', b'A', b'A', b'A', b'A', b'A',
        b'A', b'A', b'A', b'A', b'A', b'A', b'A', b'A', b'A', b'A', b'A', b'A',
        b'A', b'A', 0x00, 0x00, 0x21, 0x00, 0x01,
    ]
}

fn snmp_get_public() -> Vec<u8> {
    // SNMPv1 GetRequest, community "public", OID sysDescr.0 — universel.
    vec![
        0x30, 0x26, 0x02, 0x01, 0x00, 0x04, 0x06, b'p', b'u', b'b', b'l', b'i',
        b'c', 0xa0, 0x19, 0x02, 0x04, 0x71, 0x04, 0x8c, 0x34, 0x02, 0x01, 0x00,
        0x02, 0x01, 0x00, 0x30, 0x0b, 0x30, 0x09, 0x06, 0x05, 0x2b, 0x06, 0x01,
        0x02, 0x01, 0x05, 0x00,
    ]
}

fn ssdp_msearch() -> Vec<u8> {
    b"M-SEARCH * HTTP/1.1\r\n\
HOST: 239.255.255.250:1900\r\n\
MAN: \"ssdp:discover\"\r\n\
MX: 1\r\n\
ST: ssdp:all\r\n\r\n".to_vec()
}

fn mdns_query() -> Vec<u8> {
    // mDNS query _services._dns-sd._udp.local PTR
    vec![
        0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x09, b'_', b's', b'e', b'r', b'v', b'i', b'c', b'e', b's',
        0x07, b'_', b'd', b'n', b's', b'-', b's', b'd',
        0x04, b'_', b'u', b'd', b'p',
        0x05, b'l', b'o', b'c', b'a', b'l',
        0x00,
        0x00, 0x0c, 0x00, 0x01,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_scan_returns_all_ports() {
        let results = scan_ports("127.0.0.1", None, None).await;
        assert_eq!(results.len(), COMMON_TCP_PORTS.len());
    }
}
