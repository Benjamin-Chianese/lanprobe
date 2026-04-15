use std::path::PathBuf;
use std::process::Command;

const SPEEDTEST_WIN_URL: &str =
    "https://install.speedtest.net/app/cli/ookla-speedtest-1.2.0-win64.zip";
const SPEEDTEST_LINUX_URL: &str =
    "https://install.speedtest.net/app/cli/ookla-speedtest-1.2.0-linux-x86_64.tgz";
const DEFAULT_CACHE_DIR: &str = "/cache/speedtest";

const IPERF_WIN_URL: &str =
    "https://github.com/ar51an/iperf3-win-builds/releases/download/3.21/iperf-3.21-win64.zip";
const IPERF_DEFAULT_CACHE_DIR: &str = "/cache/iperf3";

fn main() {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    match target_os.as_str() {
        "windows" => {
            if let Err(e) = ensure_speedtest_exe() {
                println!("cargo:warning=speedtest.exe non bundlé : {e}");
            }
            if let Err(e) = ensure_iperf3_exe() {
                println!("cargo:warning=iperf3.exe non bundlé : {e}");
            }
        }
        "linux" => {
            if let Err(e) = ensure_speedtest_linux() {
                println!("cargo:warning=speedtest non bundlé (Linux) : {e}");
            }
        }
        _ => {}
    }
    println!("cargo:rerun-if-env-changed=SPEEDTEST_CACHE_DIR");
    println!("cargo:rerun-if-env-changed=IPERF3_CACHE_DIR");
    tauri_build::build()
}

fn ensure_iperf3_exe() -> Result<(), String> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let target_dir = manifest_dir.join("resources").join("win");
    let target_exe = target_dir.join("iperf3.exe");

    println!("cargo:rerun-if-changed=resources/win/iperf3.exe");

    if target_exe.exists() {
        return Ok(());
    }

    std::fs::create_dir_all(&target_dir).map_err(|e| e.to_string())?;

    let cache_dir = std::env::var("IPERF3_CACHE_DIR")
        .unwrap_or_else(|_| IPERF_DEFAULT_CACHE_DIR.to_string());
    let cached = PathBuf::from(&cache_dir).join("iperf3.exe");
    if cached.exists() {
        std::fs::copy(&cached, &target_exe).map_err(|e| e.to_string())?;
        println!("cargo:warning=iperf3.exe copié depuis le cache runner ({})", cached.display());
        return Ok(());
    }

    println!(
        "cargo:warning=cache iperf3 absent ({}) — téléchargement direct depuis GitHub (ar51an)",
        cached.display()
    );
    let zip_path = target_dir.join("iperf3-cli.zip");
    download(IPERF_WIN_URL, &zip_path)?;
    extract_zip(&zip_path, &target_dir)?;
    let _ = std::fs::remove_file(&zip_path);

    // Les releases ar51an extraient sous un sous-dossier `iperf-3.19-win64/`.
    // On cherche `iperf3.exe` n'importe où sous resources/win et on le remonte.
    if !target_exe.exists() {
        if let Some(found) = find_iperf3(&target_dir) {
            std::fs::rename(&found, &target_exe).map_err(|e| e.to_string())?;
            // Nettoie le dossier extrait s'il est vide
            if let Some(parent) = found.parent() {
                let _ = std::fs::remove_dir_all(parent);
            }
        }
    }

    if !target_exe.exists() {
        return Err(format!(
            "iperf3.exe introuvable après extraction dans {}",
            target_dir.display()
        ));
    }
    Ok(())
}

fn find_iperf3(root: &std::path::Path) -> Option<PathBuf> {
    let entries = std::fs::read_dir(root).ok()?;
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            if let Some(found) = find_iperf3(&p) {
                return Some(found);
            }
        } else if p.file_name().and_then(|n| n.to_str()) == Some("iperf3.exe") {
            return Some(p);
        }
    }
    None
}

fn ensure_speedtest_linux() -> Result<(), String> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let target_dir = manifest_dir.join("resources").join("linux");
    let target_bin = target_dir.join("speedtest");

    println!("cargo:rerun-if-changed=resources/linux/speedtest");

    if target_bin.exists() {
        return Ok(());
    }

    std::fs::create_dir_all(&target_dir).map_err(|e| e.to_string())?;

    // 1. Cache du runner CI (/cache/speedtest/speedtest)
    let cache_dir = std::env::var("SPEEDTEST_CACHE_DIR")
        .unwrap_or_else(|_| DEFAULT_CACHE_DIR.to_string());
    let cached = PathBuf::from(&cache_dir).join("speedtest");
    if cached.exists() {
        std::fs::copy(&cached, &target_bin).map_err(|e| e.to_string())?;
        println!("cargo:warning=speedtest copié depuis le cache runner ({})", cached.display());
        return Ok(());
    }

    // 2. Téléchargement depuis Ookla
    println!("cargo:warning=cache speedtest absent — téléchargement depuis Ookla");
    let tgz_path = target_dir.join("speedtest-cli.tgz");
    download(SPEEDTEST_LINUX_URL, &tgz_path)?;

    // Extraction dans un sous-dossier temporaire pour ne pas polluer resources/linux
    let tmp_dir = target_dir.join("_extract_tmp");
    let _ = std::fs::remove_dir_all(&tmp_dir);
    std::fs::create_dir_all(&tmp_dir).map_err(|e| e.to_string())?;

    let st = Command::new("tar")
        .args(["-xzf", &tgz_path.to_string_lossy(), "-C", &tmp_dir.to_string_lossy()])
        .status();
    let _ = std::fs::remove_file(&tgz_path);

    match st {
        Ok(s) if s.success() => {}
        _ => {
            let _ = std::fs::remove_dir_all(&tmp_dir);
            return Err("extraction tgz échouée".to_string());
        }
    }

    // Le binaire peut être directement dans tmp_dir ou dans un sous-dossier
    let extracted = find_file(&tmp_dir, "speedtest")
        .ok_or_else(|| format!("speedtest introuvable après extraction dans {}", tmp_dir.display()))?;

    std::fs::copy(&extracted, &target_bin).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_dir_all(&tmp_dir);

    // Rendre exécutable
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&target_bin, std::fs::Permissions::from_mode(0o755));
    }

    Ok(())
}

/// Cherche récursivement un fichier par nom dans un dossier.
fn find_file(root: &std::path::Path, name: &str) -> Option<PathBuf> {
    let entries = std::fs::read_dir(root).ok()?;
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            if let Some(found) = find_file(&p, name) {
                return Some(found);
            }
        } else if p.file_name().and_then(|n| n.to_str()) == Some(name) {
            return Some(p);
        }
    }
    None
}

fn ensure_speedtest_exe() -> Result<(), String> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let target_dir = manifest_dir.join("resources").join("win");
    let target_exe = target_dir.join("speedtest.exe");

    println!("cargo:rerun-if-changed=resources/win/speedtest.exe");

    if target_exe.exists() {
        return Ok(());
    }

    std::fs::create_dir_all(&target_dir).map_err(|e| e.to_string())?;

    // 1. Volume persistant du runner — `/cache/speedtest/speedtest.exe`
    //    seedé une fois par le job CI (cf. .gitlab-ci.yml). Évite tout
    //    accès réseau au build, et garantit la même version partout.
    let cache_dir = std::env::var("SPEEDTEST_CACHE_DIR")
        .unwrap_or_else(|_| DEFAULT_CACHE_DIR.to_string());
    let cached = PathBuf::from(&cache_dir).join("speedtest.exe");
    if cached.exists() {
        std::fs::copy(&cached, &target_exe).map_err(|e| e.to_string())?;
        println!("cargo:warning=speedtest.exe copié depuis le cache runner ({})", cached.display());
        return Ok(());
    }

    // 2. Filet de sécurité : téléchargement direct depuis Ookla.
    println!(
        "cargo:warning=cache speedtest absent ({}) — téléchargement direct depuis Ookla",
        cached.display()
    );
    let zip_path = target_dir.join("speedtest-cli.zip");
    download(SPEEDTEST_WIN_URL, &zip_path)?;
    extract_zip(&zip_path, &target_dir)?;
    let _ = std::fs::remove_file(&zip_path);

    if !target_exe.exists() {
        return Err(format!(
            "speedtest.exe introuvable après extraction dans {}",
            target_dir.display()
        ));
    }
    Ok(())
}

fn download(url: &str, dest: &std::path::Path) -> Result<(), String> {
    // curl est présent sur les images CI Linux et sur Windows 10+.
    let curl = Command::new("curl")
        .args(["-fsSL", "-o", &dest.to_string_lossy(), url])
        .status();
    if let Ok(s) = curl {
        if s.success() && dest.exists() {
            return Ok(());
        }
    }
    if cfg!(target_os = "windows") {
        let ps = Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                &format!(
                    "Invoke-WebRequest -UseBasicParsing -Uri '{}' -OutFile '{}'",
                    url,
                    dest.display()
                ),
            ])
            .status()
            .map_err(|e| e.to_string())?;
        if ps.success() {
            return Ok(());
        }
    }
    Err(format!("téléchargement échoué pour {url}"))
}

fn extract_zip(zip: &std::path::Path, dest_dir: &std::path::Path) -> Result<(), String> {
    // tar (BSD/libarchive) sait lire les .zip ; dispo sur Windows 10+ et la
    // plupart des distros Linux.
    let tar = Command::new("tar")
        .args([
            "-xf",
            &zip.to_string_lossy(),
            "-C",
            &dest_dir.to_string_lossy(),
        ])
        .status();
    if let Ok(s) = tar {
        if s.success() {
            return Ok(());
        }
    }
    let unzip = Command::new("unzip")
        .args([
            "-o",
            &zip.to_string_lossy(),
            "-d",
            &dest_dir.to_string_lossy(),
        ])
        .status();
    if let Ok(s) = unzip {
        if s.success() {
            return Ok(());
        }
    }
    Err("aucun extracteur zip disponible (tar / unzip)".to_string())
}
