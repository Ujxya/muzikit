use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::thread;
use std::time::Duration;
use anyhow::{bail, Context, Result};
use rand::RngExt;
use serde_json::Value;

use crate::library::{append_downloaded_id, load_downloaded_ids};
use crate::utils::{is_valid_video_id, safe_command, sanitize_filename, strip_control_chars};

const RETRY_MAX: u32 = 3;
const WAIT_MIN: f64 = 3.0;
const WAIT_MAX: f64 = 7.0;
/// Tek seferde kuyruğa alınabilecek en fazla şarkı (aşırı büyük/kötü niyetli playlist koruması).
const MAX_PLAYLIST_ITEMS: usize = 5000;

/// Playlist'teki tüm video ID'lerini `yt-dlp --flat-playlist -J` ile çeker.
/// Sadece geçerli 11 karakterli ASCII ID'ler döner, tekrarlar elenir.
pub fn extract_playlist_ids(url: &str) -> Result<Vec<String>> {
    let output = safe_command("yt-dlp")
        .args([
            "--ignore-config",
            "--flat-playlist",
            "-J",
            "--no-warnings",
            "--socket-timeout", "30",
            "--",
            url,
        ])
        .output()
        .context("yt-dlp çalıştırılamadı. Lütfen yt-dlp'nin kurulu olduğundan emin olun.")?;

    if !output.status.success() {
        let err = strip_control_chars(&String::from_utf8_lossy(&output.stderr));
        bail!("Playlist taranamadı: {err}");
    }

    let json: Value = serde_json::from_slice(&output.stdout)
        .context("yt-dlp çıktısı JSON olarak çözümlenemedi")?;

    let mut ids: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    if let Some(entries) = json.get("entries").and_then(|e| e.as_array()) {
        for entry in entries {
            if let Some(id) = entry.get("id").and_then(|i| i.as_str())
                && is_valid_video_id(id) && seen.insert(id.to_string()) {
                ids.push(id.to_string());
                if ids.len() >= MAX_PLAYLIST_ITEMS {
                    break;
                }
            }
        }
    }

    Ok(ids)
}

/// Hedef dosya zaten varsa üzerine yazmak yerine benzersiz bir ad üretir.
fn unique_destination(output_dir: &Path, track_num: u32, title: &str, video_id: &str) -> PathBuf {
    let primary = output_dir.join(format!("{track_num:04} - {title}.opus"));
    if fs::symlink_metadata(&primary).is_err() {
        return primary;
    }
    let with_id = output_dir.join(format!("{track_num:04} - {title} [{video_id}].opus"));
    if fs::symlink_metadata(&with_id).is_err() {
        return with_id;
    }
    let mut n = 2;
    loop {
        let candidate = output_dir.join(format!("{track_num:04} - {title} [{video_id}] ({n}).opus"));
        if fs::symlink_metadata(&candidate).is_err() {
            return candidate;
        }
        n += 1;
    }
}

/// Dosyayı, var olan hiçbir dosyanın üzerine yazmadan hedef ada taşır.
/// `rename` hedef varsa sessizce üzerine yazar; arada başka bir dosya oluşursa
/// (yarış durumu) kaybolmasın diye önce sert bağ (hard link) denenir: bu, hedef
/// varsa `AlreadyExists` ile başarısız olur ve bir sonraki benzersiz ad denenir.
fn move_no_clobber(src: &Path, output_dir: &Path, track_num: u32, title: &str, video_id: &str) -> Result<PathBuf> {
    for _ in 0..16 {
        let dest = unique_destination(output_dir, track_num, title, video_id);
        match fs::hard_link(src, &dest) {
            Ok(()) => {
                fs::remove_file(src)?;
                return Ok(dest);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            // Sert bağ desteklemeyen dosya sistemleri için klasik taşıma
            Err(_) => {
                fs::rename(src, &dest)?;
                return Ok(dest);
            }
        }
    }
    bail!("Benzersiz hedef dosya adı bulunamadı")
}

/// Tek bir videoyu indirip opus formatına çevirir ve yeniden adlandırır.
pub fn download_single(
    video_id: &str,
    track_num: u32,
    output_dir: &Path,
    logger: Option<&Sender<String>>,
) -> Result<PathBuf> {
    if !is_valid_video_id(video_id) {
        bail!("Geçersiz video kimliği reddedildi");
    }
    // URL kullanıcı girdisinden değil, doğrulanmış ID'den sabit biçimde üretilir
    let url = format!("https://www.youtube.com/watch?v={video_id}");

    // yt-dlp çıktı şablonunda `%` özel karakterdir; dizin adındaki `%` kaçışlanır
    let Some(dir_str) = output_dir.to_str() else {
        bail!("Çıktı dizini UTF-8 olmayan karakter içeriyor");
    };
    let out_template: OsString =
        OsString::from(format!("{}/{track_num:04}.%(ext)s", dir_str.replace('%', "%%")));

    let log = |msg: String| {
        let msg = strip_control_chars(&msg);
        if let Some(tx) = logger {
            let _ = tx.send(msg);
        } else {
            println!("{msg}");
        }
    };

    let mut retries = 0;
    while retries < RETRY_MAX {
        log(format!("[{track_num:04}] İndiriliyor: {url}"));

        let output = safe_command("yt-dlp")
            .arg("--ignore-config")
            .args(["-f", "bestaudio/best", "-x", "--audio-format", "opus"])
            .arg("-o")
            .arg(&out_template)
            .args([
                "--no-playlist",
                "--add-metadata",
                "--no-simulate",
                "--print", "title",
                "--",
                url.as_str(),
            ])
            .output();

        match output {
            Ok(res) if res.status.success() => {
                let stdout = String::from_utf8_lossy(&res.stdout);
                let title = stdout.lines().next().unwrap_or("Bilinmeyen").trim();
                let clean_title = sanitize_filename(title, 200);

                let expected_raw = output_dir.join(format!("{track_num:04}.opus"));

                // Sembolik bağ değil, gerçek bir dosya olmalı
                let is_regular = fs::symlink_metadata(&expected_raw)
                    .map(|m| m.file_type().is_file())
                    .unwrap_or(false);

                if is_regular {
                    let final_dest = move_no_clobber(&expected_raw, output_dir, track_num, &clean_title, video_id)?;
                    log(format!(
                        "✓ Başarılı: {}",
                        final_dest
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_default()
                    ));
                    return Ok(final_dest);
                } else {
                    log("⚠ Beklenen .opus dosyası bulunamadı.".to_string());
                    return Err(anyhow::anyhow!("Opus dosyası üretilemedi"));
                }
            }
            Ok(res) => {
                let err_msg = String::from_utf8_lossy(&res.stderr);
                let err_lower = err_msg.to_lowercase();
                // Not: düz "age" araması "page", "message", "usage" gibi kelimeleri de
                // yakalayıp geçici hataları kalıcı sanıyordu; yalnızca yaş sınırı ifadeleri aranır.
                if err_lower.contains("sign in")
                    || err_lower.contains("age-restricted")
                    || err_lower.contains("age restricted")
                    || err_lower.contains("confirm your age")
                    || err_lower.contains("private video")
                    || err_lower.contains("video unavailable")
                {
                    log(format!("Kalıcı hata (Yaş sınırı/Gizli video), atlanıyor: {video_id}"));
                    bail!("Video kalıcı olarak erişilemez: {video_id}");
                }

                retries += 1;
                log(format!(
                    "Geçici hata ({}/{}): {}",
                    retries,
                    RETRY_MAX,
                    err_msg.lines().next().unwrap_or("")
                ));
                thread::sleep(Duration::from_secs(2));
            }
            Err(e) => {
                retries += 1;
                log(format!("yt-dlp yürütme hatası ({}/{}): {}", retries, RETRY_MAX, e));
                thread::sleep(Duration::from_secs(2));
            }
        }
    }

    bail!("{RETRY_MAX} deneme başarısız oldu: {video_id}");
}

pub struct DownloadReport {
    pub failed: usize,
}

/// Kurulu yt-dlp sürümünü döner (örn: `2026.10.26`).
pub fn yt_dlp_version() -> Option<String> {
    let out = safe_command("yt-dlp")
        .args(["--ignore-config", "--version"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let ver = strip_control_chars(String::from_utf8_lossy(&out.stdout).trim());
    if ver.is_empty() { None } else { Some(ver) }
}

/// yt-dlp'yi kendi yerleşik `--update` (`-U`) mekanizmasıyla günceller (sudo gerektirmez).
pub fn update_yt_dlp(logger: Option<&Sender<String>>) -> Result<()> {
    let log = |msg: String| {
        let msg = strip_control_chars(&msg);
        if let Some(tx) = logger {
            let _ = tx.send(msg);
        } else {
            println!("{msg}");
        }
    };

    if let Some(ver) = yt_dlp_version() {
        log(format!("Mevcut yt-dlp surumu: {ver}"));
    }
    log("yt-dlp guncellemesi kontrol ediliyor (yt-dlp -U)...".to_string());

    let output = safe_command("yt-dlp")
        .args(["--ignore-config", "-U"])
        .output()
        .context("yt-dlp çalıştırılamadı. ~/.local/bin/yt-dlp mevcut mu?")?;

    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let clean = strip_control_chars(line.trim());
        if !clean.is_empty() {
            log(clean);
        }
    }
    for line in String::from_utf8_lossy(&output.stderr).lines() {
        let clean = strip_control_chars(line.trim());
        if !clean.is_empty() {
            log(clean);
        }
    }

    if output.status.success() {
        if let Some(new_ver) = yt_dlp_version() {
            log(format!("✓ Basarili: Guncel yt-dlp surumu: {new_ver}"));
        } else {
            log("✓ Basarili: yt-dlp guncelleme tamamlandi.".to_string());
        }
        Ok(())
    } else {
        bail!("yt-dlp guncellenemedi (cikis kodu: {:?})", output.status.code());
    }
}

/// Kuyruktaki parçaları sırayla indirir.
pub fn process_download_queue(
    ids: Vec<String>,
    output_dir: &Path,
    download_log: &Path,
    mut next_num: u32,
    logger: Option<&Sender<String>>,
) -> Result<DownloadReport> {
    let log = |msg: String| {
        if let Some(tx) = logger {
            let _ = tx.send(msg);
        } else {
            println!("{msg}");
        }
    };

    let downloaded_ids = load_downloaded_ids(download_log)?;
    let mut seen = std::collections::HashSet::new();
    let new_ids: Vec<String> = ids
        .into_iter()
        .filter(|id| is_valid_video_id(id))
        .filter(|id| !downloaded_ids.contains(id))
        .filter(|id| seen.insert(id.clone()))
        .take(MAX_PLAYLIST_ITEMS)
        .collect();

    if new_ids.is_empty() {
        log("ℹ Şarkıların tümü zaten indirilmiş.".to_string());
        return Ok(DownloadReport { failed: 0 });
    }

    log(format!("📋 {} yeni şarkı sıraya alındı.\n", new_ids.len()));

    let mut rng = rand::rng();
    let mut failed = 0usize;

    for (idx, vid) in new_ids.iter().enumerate() {
        log(format!("--- [{}/{}] ---", idx + 1, new_ids.len()));

        match download_single(vid, next_num, output_dir, logger) {
            Ok(path) => {
                let _ = append_downloaded_id(download_log, vid);
                // Eşitleme defteri: videoId bilinsin (hata olursa tarama yine bulur)
                let _ = crate::sync::ledger::record_download(output_dir, &path, vid);
                next_num += 1;

                if idx + 1 < new_ids.len() {
                    let wait_secs = rng.random_range(WAIT_MIN..=WAIT_MAX);
                    log(format!("⏳ Kota koruması: {:.1} sn bekleniyor...", wait_secs));
                    thread::sleep(Duration::from_millis((wait_secs * 1000.0) as u64));
                }
            }
            Err(e) => {
                failed += 1;
                log(strip_control_chars(&format!("❌ Atlandı: {vid} ({e})")));
            }
        }
    }

    log("✨ Tüm indirme işlemleri tamamlandı!".to_string());
    Ok(DownloadReport { failed })
}
