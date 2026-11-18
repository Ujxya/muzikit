use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use anyhow::{Context, Result};
use crate::utils::parse_track_number;

/// downloaded.txt dosyasından daha önce indirilmiş YouTube ID'lerini yükler.
pub fn load_downloaded_ids<P: AsRef<Path>>(filepath: P) -> Result<HashSet<String>> {
    let path = filepath.as_ref();
    if !path.exists() {
        return Ok(HashSet::new());
    }

    let file = fs::File::open(path)
        .with_context(|| format!("{} dosyası açılamadı", path.display()))?;
    let reader = BufReader::new(file);

    let ids = reader
        .lines()
        .map_while(Result::ok)
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();

    Ok(ids)
}

/// İndirilen ID'yi downloaded.txt dosyasına ekler.
pub fn append_downloaded_id<P: AsRef<Path>>(filepath: P, video_id: &str) -> Result<()> {
    // Satır enjeksiyonunu (\n) ve bozuk kayıtları engelle
    if !crate::utils::is_valid_video_id(video_id) {
        anyhow::bail!("Geçersiz video kimliği downloaded.txt'ye yazılmadı");
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(filepath)?;

    writeln!(file, "{video_id}")?;
    Ok(())
}

/// Dizin içindeki en büyük şarkı numarasını bulup bir sonraki numarayı döner.
pub fn get_next_number<P: AsRef<Path>>(directory: P) -> u32 {
    let mut max_num = 0;
    if let Ok(entries) = fs::read_dir(directory) {
        for entry in entries.flatten() {
            if let Ok(file_type) = entry.file_type()
                && file_type.is_file() {
                let file_name = entry.file_name();
                let name_str = file_name.to_string_lossy();
                if name_str.ends_with(".opus")
                    && let Some(num) = parse_track_number(&name_str)
                        && num > max_num {
                            max_num = num;
                        }
            }
        }
    }
    max_num + 1
}

/// Kütüphane sağlık kontrolü: Eksik numara var mı, kaç parça var vb. raporlar.
pub struct LibraryStats {
    pub total_opus_files: usize,
    pub max_number: u32,
    pub missing_numbers: Vec<u32>,
    pub downloaded_txt_count: usize,
}

pub fn inspect_library<P: AsRef<Path>>(dir: P, downloaded_file: P) -> Result<LibraryStats> {
    let dir_path = dir.as_ref();
    let downloaded_ids = load_downloaded_ids(downloaded_file)?;
    
    let mut found_numbers = Vec::new();
    let mut total_opus = 0;

    if dir_path.exists() {
        for entry in fs::read_dir(dir_path)?.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.ends_with(".opus") {
                total_opus += 1;
                if let Some(num) = parse_track_number(&name) {
                    found_numbers.push(num);
                }
            }
        }
    }

    found_numbers.sort_unstable();
    let max_number = found_numbers.last().copied().unwrap_or(0);

    let mut missing_numbers = Vec::new();
    if max_number > 0 {
        let set: HashSet<u32> = found_numbers.into_iter().collect();
        for n in 1..=max_number {
            if !set.contains(&n) {
                missing_numbers.push(n);
            }
        }
    }

    Ok(LibraryStats {
        total_opus_files: total_opus,
        max_number,
        missing_numbers,
        downloaded_txt_count: downloaded_ids.len(),
    })
}
