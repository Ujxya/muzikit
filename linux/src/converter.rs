use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use anyhow::{bail, Context, Result};

use crate::utils::{is_valid_bitrate, safe_command, strip_control_chars};

/// FFmpeg'e verilecek yolu güvenli hale getirir: `-` ile başlayan veya `http:`/`concat:` gibi
/// protokol öneki taşıyabilecek göreli yollar `./` ile sabitlenir.
fn ffmpeg_safe_path(p: &Path) -> PathBuf {
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        Path::new(".").join(p)
    }
}

/// Tek bir MP3 dosyasını FFmpeg kullanarak optimize Opus formatına dönüştürür.
/// Var olan çıktı dosyasının üzerine YAZMAZ (`-n`).
pub fn mp3_to_opus(
    input_file: &Path,
    output_file: &Path,
    bitrate: &str,
) -> Result<(f64, f64, f64)> {
    if !is_valid_bitrate(bitrate) {
        bail!("Geçersiz bitrate: '{}' (örn: 96k, 128k, 160k)", strip_control_chars(bitrate));
    }

    let output = safe_command("ffmpeg")
        .args(["-nostdin", "-hide_banner", "-loglevel", "error", "-n"])
        // Girdi sadece yerel dosya ve sadece MP3 olarak okunur: .mp3 adlı bir HLS/concat listesi
        // ffmpeg'e başka dosyaları okutamaz ya da ağa istek attıramaz
        .args(["-protocol_whitelist", "file", "-f", "mp3"])
        .arg("-i")
        .arg(ffmpeg_safe_path(input_file))
        .args([
            "-vn",
            "-af", "highpass=20,lowpass=20000",
            "-map_metadata", "-1",
            "-map_metadata:s", "-1",
            "-c:a", "libopus",
            "-b:a", bitrate,
            "-application", "voip",
            "-frame_duration", "20",
        ])
        .arg(ffmpeg_safe_path(output_file))
        .output()
        .context("FFmpeg çalıştırılamadı")?;

    if !output.status.success() {
        let err = strip_control_chars(&String::from_utf8_lossy(&output.stderr));
        bail!("FFmpeg dönüştürme hatası: {err}");
    }

    let input_bytes = fs::metadata(input_file)?.len() as f64;
    let output_bytes = fs::metadata(output_file)?.len() as f64;

    let input_mb = input_bytes / (1024.0 * 1024.0);
    let output_mb = output_bytes / (1024.0 * 1024.0);
    let compression = if input_bytes > 0.0 {
        ((input_bytes - output_bytes) / input_bytes) * 100.0
    } else {
        0.0
    };

    Ok((input_mb, output_mb, compression))
}

/// Bir klasördeki tüm MP3 dosyalarını tarayıp topluca Opus formatına dönüştürür.
pub fn batch_convert(
    input_dir: &Path,
    output_dir: &Path,
    bitrate: &str,
    logger: Option<&Sender<String>>,
) -> Result<()> {
    let log = |msg: String| {
        let msg = strip_control_chars(&msg);
        if let Some(tx) = logger {
            let _ = tx.send(msg);
        } else {
            println!("{msg}");
        }
    };

    if !is_valid_bitrate(bitrate) {
        bail!("Geçersiz bitrate: '{}' (örn: 96k, 128k, 160k)", strip_control_chars(bitrate));
    }

    if !input_dir.is_dir() {
        bail!("Girdi klasörü bulunamadı: {}", input_dir.display());
    }

    fs::create_dir_all(output_dir)?;

    let mut mp3_files: Vec<PathBuf> = Vec::new();
    for entry in fs::read_dir(input_dir)?.flatten() {
        // Sembolik bağlar takip edilmez; sadece gerçek dosyalar işlenir
        let Ok(ft) = entry.file_type() else { continue };
        if !ft.is_file() {
            continue;
        }
        let path = entry.path();
        if let Some(ext) = path.extension()
            && ext.to_string_lossy().eq_ignore_ascii_case("mp3") {
            mp3_files.push(path);
        }
    }

    mp3_files.sort();

    if mp3_files.is_empty() {
        log("ℹ Belirtilen dizinde MP3 dosyası bulunamadı.".to_string());
        return Ok(());
    }

    log(format!(
        "🎵 {} MP3 dosyası dönüştürülüyor... (Hedef: {})",
        mp3_files.len(),
        bitrate
    ));

    let mut success_count = 0;
    let mut total_saved_mb = 0.0;

    for (i, mp3_path) in mp3_files.iter().enumerate() {
        let file_stem = mp3_path.file_stem().unwrap_or_default();
        let mut out_name = file_stem.to_os_string();
        out_name.push(".opus");
        let out_file = output_dir.join(out_name);
        let display_name = mp3_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        // Var olan (veya sembolik bağ olan) hedefin üzerine yazılmaz
        if fs::symlink_metadata(&out_file).is_ok() {
            log(format!(
                "↷ [{}/{}] Atlandı (hedef zaten var): {}",
                i + 1,
                mp3_files.len(),
                display_name
            ));
            continue;
        }

        match mp3_to_opus(mp3_path, &out_file, bitrate) {
            Ok((in_mb, out_mb, comp)) => {
                success_count += 1;
                total_saved_mb += in_mb - out_mb;
                let change = if comp >= 0.0 { format!("%{comp:.1} küçüldü") } else { format!("%{:.1} büyüdü", -comp) };
                log(format!(
                    "✓ [{}/{}] {} ({:.2}MB → {:.2}MB, {change})",
                    i + 1,
                    mp3_files.len(),
                    display_name,
                    in_mb,
                    out_mb,
                ));
            }
            Err(e) => {
                log(format!(
                    "✗ [{}/{}] Hata: {}: {}",
                    i + 1,
                    mp3_files.len(),
                    display_name,
                    e
                ));
            }
        }
    }

    let total = if total_saved_mb >= 0.0 {
        format!("Toplam tasarruf: {total_saved_mb:.2} MB")
    } else {
        format!("Toplam artış: {:.2} MB", -total_saved_mb)
    };
    log(format!("✓ Tamamlandı! {success_count} dosya dönüştürüldü. {total}"));

    Ok(())
}
