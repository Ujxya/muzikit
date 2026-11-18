use std::path::{Path, PathBuf};

pub struct Config {
    pub music_dir: PathBuf,
    pub log_file: PathBuf,
}

impl Config {
    pub fn default_detected() -> Self {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        let home_path = Path::new(&home);

        // Kullanıcının arşivinin olduğu bilinen yollar
        let candidates = [
            home_path.join("Müzik/Müzik"),
            home_path.join("Müzik"),
            home_path.join("Music"),
            PathBuf::from("."),
        ];

        let music_dir = candidates
            .into_iter()
            .find(|p| p.exists() && p.is_dir())
            .unwrap_or_else(|| PathBuf::from("."));

        let log_file = music_dir.join("downloaded.txt");

        Self {
            music_dir,
            log_file,
        }
    }
}
