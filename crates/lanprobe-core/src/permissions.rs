#[cfg(target_os = "macos")]
use super::proc::sync_cmd;

#[cfg(target_os = "macos")]
const SUDOERS_PATH: &str = "/etc/sudoers.d/lanprobe";

/// Vérifie si la configuration privilèges est en place.
/// Sur Linux et Windows il n'y a rien à installer (ping unprivilégié via
/// SOCK_DGRAM/ping.exe, pas de networksetup), donc on renvoie directement
/// `true` — sinon la bannière de setup s'affichait à chaque démarrage.
pub fn has_permissions() -> bool {
    #[cfg(target_os = "macos")]
    { std::path::Path::new(SUDOERS_PATH).exists() }
    #[cfg(not(target_os = "macos"))]
    { true }
}

/// Installe la règle sudoers via AppleScript (demande le mot de passe UNE FOIS).
/// Après ça, networksetup et ping fonctionnent sans password.
#[cfg(target_os = "macos")]
pub fn install_permissions() -> Result<(), String> {
    // Le groupe %admin couvre tous les utilisateurs admins macOS
    // Seulement networksetup — le ping est géré via SOCK_DGRAM (pas besoin de root)
    let rule = "%admin ALL=(root) NOPASSWD: /usr/sbin/networksetup";
    let script = format!(
        r#"do shell script "printf '{}\\n' > {SUDOERS_PATH} && chmod 440 {SUDOERS_PATH}" with administrator privileges"#,
        rule
    );
    let out = sync_cmd("osascript")
        .args(["-e", &script])
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).into_owned();
        if err.contains("User cancelled") || err.is_empty() {
            return Err("Opération annulée".to_string());
        }
        return Err(err);
    }
    Ok(())
}

#[cfg(not(target_os = "macos"))]
pub fn install_permissions() -> Result<(), String> {
    Ok(()) // Linux/Windows : pas besoin de cette étape
}

/// Exécute une commande réseau avec sudo -n (sans mot de passe si sudoers installé),
/// sinon via AppleScript (avec prompt password).
#[cfg(target_os = "macos")]
pub fn run_privileged(cmd_args: &[&str]) -> Result<String, String> {
    if has_permissions() {
        // sudo -n = non-interactif, échoue proprement si pas de règle
        let out = sync_cmd("sudo")
            .arg("-n")
            .args(cmd_args)
            .output()
            .map_err(|e| e.to_string())?;
        if out.status.success() {
            return Ok(String::from_utf8_lossy(&out.stdout).into_owned());
        }
    }
    // Fallback AppleScript
    let cmd_str = cmd_args.join(" ");
    let script = format!(r#"do shell script "{}" with administrator privileges"#, cmd_str);
    let out = sync_cmd("osascript")
        .args(["-e", &script])
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).into_owned());
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}
