//! `.muzikit-index.tsv` defteri. Defter bir ÖNBELLEKTİR:
//! her eşitlemeden önce klasör taranır, eksik satırlar eklenir, dosyası olmayanlar düşülür.
//! `size`+`mtime` değişmeyen dosyanın SHA-256'sı yeniden hesaplanmaz.

use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};

use crate::utils::{is_valid_video_id, parse_track_number};

pub const INDEX_FILE: &str = ".muzikit-index.tsv";
const HEADER: &str = "# muzikit-index v1\nnumber\tsha256\tsize\tmtime\tvideoId\taddedAt\taddedBy\tname\n";
pub const DEVICE: &str = "pc";

#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub number: u32,
    pub sha256: String,
    pub size: u64,
    pub mtime: i64,
    pub video_id: Option<String>,
    pub added_at: String,
    pub added_by: String,
    pub name: String,
}

/// Unix saniyesini `YYYY-MM-DDTHH:MM:SSZ` (UTC) biçimine çevirir.
pub fn iso_utc(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    // Howard Hinnant'ın civil_from_days algoritması
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", rem / 3600, rem % 3600 / 60, rem % 60)
}

pub fn is_valid_iso(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 20
        && b.iter().enumerate().all(|(i, c)| match i {
            4 | 7 => *c == b'-',
            10 => *c == b'T',
            13 | 16 => *c == b':',
            19 => *c == b'Z',
            _ => c.is_ascii_digit(),
        })
}

pub fn now_secs() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

pub fn is_sha_hex(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|c| matches!(c, b'0'..=b'9' | b'a'..=b'f'))
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn sha256_file(path: &Path) -> Result<String> {
    let mut f = fs::File::open(path).with_context(|| format!("{} açılamadı", path.display()))?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(hex(&h.finalize()))
}

fn mtime_of(meta: &fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Defterde tutulabilecek ad: şarkı deseni, sekme/satır sonu yok.
pub fn is_song_name(name: &str) -> bool {
    parse_track_number(name).is_some() && !name.chars().any(|c| c.is_control())
}

fn parse_line(line: &str) -> Option<Entry> {
    let f: Vec<&str> = line.split('\t').collect();
    if f.len() != 8 {
        return None;
    }
    let name = f[7].to_string();
    let number = parse_track_number(&name)?;
    if f[0].parse::<u32>().ok()? != number || !is_sha_hex(f[1]) || !is_valid_iso(f[5]) || f[6].is_empty() {
        return None;
    }
    Some(Entry {
        number,
        sha256: f[1].to_string(),
        size: f[2].parse().ok()?,
        mtime: f[3].parse().ok()?,
        video_id: (f[4] != "-" && is_valid_video_id(f[4])).then(|| f[4].to_string()),
        added_at: f[5].to_string(),
        added_by: f[6].to_string(),
        name,
    })
}

/// Defteri okur; bozuk satırlar atlanır (defter önbellek, tarama onları yeniden kurar).
pub fn load(dir: &Path) -> Vec<Entry> {
    fs::read_to_string(dir.join(INDEX_FILE))
        .map(|s| s.lines().filter(|l| !l.starts_with('#')).filter_map(parse_line).collect())
        .unwrap_or_default()
}

/// Defteri atomik olarak yazar (geçici dosya + rename).
pub fn save(dir: &Path, entries: &[Entry]) -> Result<()> {
    let mut out = String::from(HEADER);
    for e in entries {
        out.push_str(&format!(
            "{:04}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
            e.number,
            e.sha256,
            e.size,
            e.mtime,
            e.video_id.as_deref().unwrap_or("-"),
            e.added_at,
            e.added_by,
            e.name
        ));
    }
    let tmp = dir.join(format!("{INDEX_FILE}.tmp"));
    {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(out.as_bytes())?;
        f.sync_all()?;
    }
    fs::rename(&tmp, dir.join(INDEX_FILE))?;
    Ok(())
}

/// Klasörü tarar, defteri tamamlar ve kaydeder. `progress(yapılan, toplam)` hash sırasında çağrılır.
pub fn refresh(dir: &Path, mut progress: impl FnMut(usize, usize)) -> Result<Vec<Entry>> {
    let old = load(dir);
    let by_name: HashMap<&str, &Entry> = old.iter().map(|e| (e.name.as_str(), e)).collect();
    // Yeniden adlandırılmış dosya: içerik aynıysa ekleme bilgisi korunur
    let by_sha: HashMap<&str, &Entry> = old.iter().map(|e| (e.sha256.as_str(), e)).collect();

    let mut files: Vec<(String, PathBuf, fs::Metadata)> = Vec::new();
    for ent in fs::read_dir(dir).with_context(|| format!("{} okunamadı", dir.display()))?.flatten() {
        let Ok(meta) = ent.metadata() else { continue };
        // Sembolik bağlar izlenmez (klasör dışına yazma/okuma riski)
        let Ok(ft) = ent.file_type() else { continue };
        if !ft.is_file() {
            continue;
        }
        let Some(name) = ent.file_name().to_str().map(str::to_string) else { continue };
        if is_song_name(&name) {
            files.push((name, ent.path(), meta));
        }
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));

    let total = files.len();
    let mut out = Vec::with_capacity(total);
    for (i, (name, path, meta)) in files.into_iter().enumerate() {
        progress(i, total);
        let size = meta.len();
        let mtime = mtime_of(&meta);
        let number = parse_track_number(&name).unwrap_or(0);
        if let Some(e) = by_name.get(name.as_str()).filter(|e| e.size == size && e.mtime == mtime) {
            out.push((*e).clone());
            continue;
        }
        let sha256 = sha256_file(&path)?;
        let prev = by_sha.get(sha256.as_str());
        out.push(Entry {
            number,
            size,
            mtime,
            video_id: prev.and_then(|p| p.video_id.clone()),
            added_at: prev.map(|p| p.added_at.clone()).unwrap_or_else(|| iso_utc(mtime)),
            added_by: prev.map(|p| p.added_by.clone()).unwrap_or_else(|| DEVICE.to_string()),
            sha256,
            name,
        });
    }
    progress(total, total);
    out.sort_by(|a, b| (a.number, &a.name).cmp(&(b.number, &b.name)));
    save(dir, &out)?;
    Ok(out)
}

/// İndirici yeni şarkıyı kaydeder (videoId bilinsin diye). Hata eşitlemeyi bozmaz: tarama yine bulur.
pub fn record_download(dir: &Path, path: &Path, video_id: &str) -> Result<()> {
    let name = path.file_name().and_then(|n| n.to_str()).context("geçersiz ad")?.to_string();
    if !is_song_name(&name) {
        return Ok(());
    }
    let meta = fs::metadata(path)?;
    let mut entries = load(dir);
    entries.retain(|e| e.name != name);
    entries.push(Entry {
        number: parse_track_number(&name).unwrap_or(0),
        sha256: sha256_file(path)?,
        size: meta.len(),
        mtime: mtime_of(&meta),
        video_id: is_valid_video_id(video_id).then(|| video_id.to_string()),
        added_at: iso_utc(now_secs()),
        added_by: DEVICE.to_string(),
        name,
    });
    entries.sort_by(|a, b| (a.number, &a.name).cmp(&(b.number, &b.name)));
    save(dir, &entries)
}

/// Var olan bir dosyanın defter satırını, verilen ekleme bilgisiyle yazar/günceller.
pub fn upsert(dir: &Path, path: &Path, sha256: &str, video_id: Option<String>, added_at: &str, added_by: &str) -> Result<()> {
    let name = path.file_name().and_then(|n| n.to_str()).context("geçersiz ad")?.to_string();
    let meta = fs::metadata(path)?;
    let mut entries = load(dir);
    entries.retain(|e| e.name != name);
    entries.push(Entry {
        number: parse_track_number(&name).context("şarkı adı değil")?,
        sha256: sha256.to_string(),
        size: meta.len(),
        mtime: mtime_of(&meta),
        video_id,
        added_at: added_at.to_string(),
        added_by: added_by.to_string(),
        name,
    });
    entries.sort_by(|a, b| (a.number, &a.name).cmp(&(b.number, &b.name)));
    save(dir, &entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso_dates() {
        assert_eq!(iso_utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(iso_utc(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(iso_utc(1_763_251_200 + 3661), "2025-11-16T01:01:01Z");
        assert!(is_valid_iso(&iso_utc(now_secs())));
        assert!(!is_valid_iso("2026-11-16 10:00:00Z"));
    }

    #[test]
    fn refresh_scans_caches_and_drops() {
        let dir = std::env::temp_dir().join(format!("muzikit-ledger-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("0001 - a.opus"), b"aaa").unwrap();
        fs::write(dir.join("0002 - b.opus"), b"bbb").unwrap();
        fs::write(dir.join("notes.txt"), b"x").unwrap();
        fs::write(dir.join(".0003.part"), b"x").unwrap();
        let e = refresh(&dir, |_, _| {}).unwrap();
        assert_eq!(e.len(), 2);
        assert_eq!(e[0].sha256, "9834876dcfb05cb167a5c24953eba58c4ac89b1adf57f28f2f9d09af107ee8f0");
        assert_eq!(e[0].added_by, "pc");

        // Rename: ekleme bilgisi içerikten korunur; silinen dosya düşer
        upsert(&dir, &dir.join("0001 - a.opus"), &e[0].sha256, Some("dQw4w9WgXcQ".into()), "2020-01-01T00:00:00Z", "phone").unwrap();
        fs::rename(dir.join("0001 - a.opus"), dir.join("0005 - a.opus")).unwrap();
        fs::remove_file(dir.join("0002 - b.opus")).unwrap();
        let e = refresh(&dir, |_, _| {}).unwrap();
        assert_eq!(e.len(), 1);
        assert_eq!((e[0].number, e[0].added_by.as_str()), (5, "phone"));
        assert_eq!(e[0].video_id.as_deref(), Some("dQw4w9WgXcQ"));
        assert_eq!(load(&dir), e);
        fs::remove_dir_all(&dir).unwrap();
    }
}
