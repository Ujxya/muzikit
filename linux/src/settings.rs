//! GUI ayarlarının kalıcı olarak saklanması (~/.config/muzikit/gui.json).

use std::collections::HashMap;
use std::path::PathBuf;

use crate::keymap::StoredConf;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Settings {
    /// Kullanıcının elle seçtiği müzik klasörü (yoksa otomatik tespit)
    pub music_dir: Option<PathBuf>,
    pub volume: u32,
    pub shuffle: bool,
    /// "off" | "all" | "one"
    pub loop_mode: String,
    pub favorites: Vec<String>,
    pub last_track: Option<String>,
    pub bitrate: String,
    pub sort: String,
    /// Tüm klavye kısayollarının ana anahtarı
    pub shortcuts_enabled: bool,
    /// Eylem kimliği → (açık mı, tuşlar)
    pub shortcuts: HashMap<String, StoredConf>,
    /// Kayıtlı kısayol biçiminin sürümü; eski dosyalarda yoktur (0) ve geçiş yapılır
    #[serde(default)]
    pub shortcuts_version: u32,
    /// Hareketli arka plan (radar, veri akışı ...)
    pub fx_enabled: bool,
    /// Menü/panel zemininin opaklığı (0 = tam saydam, 255 = siyah)
    pub panel_alpha: u8,
    /// Arka plan teması: "siber" | "balik"
    pub theme: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            music_dir: None,
            volume: 100,
            shuffle: false,
            loop_mode: "off".to_string(),
            favorites: Vec::new(),
            last_track: None,
            bitrate: "128k".to_string(),
            sort: "number".to_string(),
            shortcuts_enabled: true,
            shortcuts: HashMap::new(),
            shortcuts_version: crate::keymap::SHORTCUTS_VERSION,
            fx_enabled: true,
            panel_alpha: 125,
            theme: "siber".to_string(),
        }
    }
}

fn settings_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                // Göreli/boş HOME ile ayarlar çalışma dizinine yazılmasın
                .filter(|h| h.is_absolute())
                .map(|h| h.join(".config"))
        })?;
    Some(base.join("muzikit").join("gui.json"))
}

impl Settings {
    pub fn load() -> Self {
        let Some(path) = settings_path() else {
            return Self::default();
        };
        match std::fs::read_to_string(&path) {
            // Bozuk dosya uygulamayı çökertmesin; varsayılanlarla devam
            Ok(text) if text.len() < 4 * 1024 * 1024 => serde_json::from_str(&text).unwrap_or_default(),
            _ => Self::default(),
        }
    }

    pub fn save(&self) {
        let Some(path) = settings_path() else {
            return;
        };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(text) = serde_json::to_string_pretty(self) {
            // Yarım yazılmış dosya kalmasın diye önce geçici dosyaya yaz, sonra taşı
            let tmp = path.with_extension("json.tmp");
            if std::fs::write(&tmp, text).is_ok() {
                let _ = std::fs::rename(&tmp, &path);
            }
        }
    }
}
