use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::net::{IpAddr, Ipv4Addr};

#[derive(Debug, Serialize, Clone, Default)]
pub struct PublicIpInfo {
    pub ip: String,
    pub country: Option<String>,
    pub city: Option<String>,
    pub isp: Option<String>,
}

#[derive(Deserialize)]
struct IpApiResponse {
    #[serde(default)]
    query: String,
    #[serde(default)]
    country: Option<String>,
    #[serde(default)]
    city: Option<String>,
    #[serde(default)]
    isp: Option<String>,
}

/// Récupère l'IP publique + géoloc via ip-api.com. Utilise l'interface
/// sélectionnée comme `local_address` pour que la requête sorte bien par
/// la bonne carte, sinon elle emprunte la route par défaut.
pub async fn get_public_ip(src: Option<Ipv4Addr>) -> Result<PublicIpInfo, String> {
    let mut builder = Client::builder().timeout(std::time::Duration::from_secs(8));
    if let Some(s) = src {
        builder = builder.local_address(IpAddr::V4(s));
    }
    let client = builder.build().map_err(|e| e.to_string())?;

    // ip-api.com : champs optionnels, gratuit, pas de clé
    let resp = client
        .get("http://ip-api.com/json/?fields=query,country,city,isp")
        .send().await.map_err(|e| e.to_string())?;
    let r: IpApiResponse = resp.json().await.map_err(|e| e.to_string())?;

    if r.query.is_empty() {
        return Err("réponse ip-api vide".into());
    }

    Ok(PublicIpInfo {
        ip: r.query,
        country: r.country,
        city: r.city,
        isp: r.isp,
    })
}
