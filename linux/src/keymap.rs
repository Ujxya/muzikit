//! Yeniden atanabilir, tek tek açılıp kapatılabilen klavye kısayolları.
//!
//! İki tür tuş ataması vardır:
//! - `Char`: yazı üreten tuşlar (harf, rakam, `+`, `/`, `?` ...). Metin olayından okunur;
//!   böylece klavye düzeninden bağımsızdır ve büyük/küçük harf ayrımı vardır (`f` ≠ `F`).
//! - `Key`: yazı üretmeyen tuşlar (Space, ←/→, F1–F12, Insert ...) ya da Ctrl/Alt'lı
//!   kombinasyonlar. Değiştirici tuşlar (Ctrl/Alt/Shift) birebir eşleşmelidir.

use std::collections::HashMap;

use eframe::egui::{Key, Modifiers};
use serde::{Deserialize, Serialize};

/// Bir eylem en fazla bu kadar tuşa atanabilir.
pub const SLOTS: usize = 2;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Action {
    PlayPause,
    Next,
    Prev,
    SeekBack,
    SeekFwd,
    SeekBackLong,
    SeekFwdLong,
    VolUp,
    VolDown,
    Mute,
    Stop,
    Shuffle,
    Loop,
    Sleep,
    Search,
    Favorite,
    OnlyFavorites,
    Enqueue,
    PlayNextInQueue,
    RevealPlaying,
    Refresh,
    OpenDir,
    UpdateYtdlp,
    ScreenDownload,
    ScreenConvert,
    ScreenLibrary,
    ScreenSettings,
    Quit,
    Help,
    ToggleFx,
}

/// Eylemin geçerli olduğu ekran(lar).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Scope {
    Global,
    Library,
    MainMenu,
    DownloaderOrSettings,
}

impl Action {
    pub const ALL: [Action; 30] = [
        Action::PlayPause,
        Action::Next,
        Action::Prev,
        Action::SeekBack,
        Action::SeekFwd,
        Action::SeekBackLong,
        Action::SeekFwdLong,
        Action::VolUp,
        Action::VolDown,
        Action::Mute,
        Action::Stop,
        Action::Shuffle,
        Action::Loop,
        Action::Sleep,
        Action::Search,
        Action::Favorite,
        Action::OnlyFavorites,
        Action::Enqueue,
        Action::PlayNextInQueue,
        Action::RevealPlaying,
        Action::Refresh,
        Action::OpenDir,
        Action::UpdateYtdlp,
        Action::ScreenDownload,
        Action::ScreenConvert,
        Action::ScreenLibrary,
        Action::ScreenSettings,
        Action::Quit,
        Action::Help,
        Action::ToggleFx,
    ];

    /// Ayar dosyasında kullanılan kalıcı kimlik (değiştirilmemeli).
    pub fn id(self) -> &'static str {
        match self {
            Action::PlayPause => "play_pause",
            Action::Next => "next",
            Action::Prev => "prev",
            Action::SeekBack => "seek_back",
            Action::SeekFwd => "seek_fwd",
            Action::SeekBackLong => "seek_back_long",
            Action::SeekFwdLong => "seek_fwd_long",
            Action::VolUp => "vol_up",
            Action::VolDown => "vol_down",
            Action::Mute => "mute",
            Action::Stop => "stop",
            Action::Shuffle => "shuffle",
            Action::Loop => "loop",
            Action::Sleep => "sleep",
            Action::Search => "search",
            Action::Favorite => "favorite",
            Action::OnlyFavorites => "only_favorites",
            Action::Enqueue => "enqueue",
            Action::PlayNextInQueue => "play_next_in_queue",
            Action::RevealPlaying => "reveal_playing",
            Action::Refresh => "refresh",
            Action::OpenDir => "open_dir",
            Action::UpdateYtdlp => "update_ytdlp",
            Action::ScreenDownload => "screen_download",
            Action::ScreenConvert => "screen_convert",
            Action::ScreenLibrary => "screen_library",
            Action::ScreenSettings => "screen_settings",
            Action::Quit => "quit",
            Action::Help => "help",
            Action::ToggleFx => "toggle_fx",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Action::PlayPause => "dur / devam",
            Action::Next => "sonraki sarki",
            Action::Prev => "onceki sarki",
            Action::SeekBack => "5 sn geri sar",
            Action::SeekFwd => "5 sn ileri sar",
            Action::SeekBackLong => "30 sn geri sar",
            Action::SeekFwdLong => "30 sn ileri sar",
            Action::VolUp => "ses +%5",
            Action::VolDown => "ses -%5",
            Action::Mute => "sessiz",
            Action::Stop => "durdur",
            Action::Shuffle => "karistir",
            Action::Loop => "dongu: kapali > liste > tek",
            Action::Sleep => "uyku zamanlayicisi",
            Action::Search => "ara",
            Action::Favorite => "favori ekle / cikar",
            Action::OnlyFavorites => "sadece favoriler",
            Action::Enqueue => "siraya ekle",
            Action::PlayNextInQueue => "siradaki yap",
            Action::RevealPlaying => "calani listede goster",
            Action::Refresh => "kutuphaneyi yenile",
            Action::OpenDir => "muzik klasorunu ac",
            Action::UpdateYtdlp => "yt-dlp guncelle",
            Action::ScreenDownload => "ekran: muzik indir",
            Action::ScreenConvert => "ekran: mp3 > opus",
            Action::ScreenLibrary => "ekran: kutuphane",
            Action::ScreenSettings => "ekran: ayarlar & kisayollar",
            Action::Quit => "cikis (ana menude)",
            Action::Help => "kisayol yardimi",
            Action::ToggleFx => "arka plan efektleri",
        }
    }

    pub fn group(self) -> &'static str {
        match self {
            Action::PlayPause
            | Action::Next
            | Action::Prev
            | Action::SeekBack
            | Action::SeekFwd
            | Action::SeekBackLong
            | Action::SeekFwdLong
            | Action::VolUp
            | Action::VolDown
            | Action::Mute
            | Action::Stop
            | Action::Shuffle
            | Action::Loop
            | Action::Sleep => "calma",
            Action::Search
            | Action::Favorite
            | Action::OnlyFavorites
            | Action::Enqueue
            | Action::PlayNextInQueue
            | Action::RevealPlaying
            | Action::Refresh => "kutuphane",
            Action::ScreenDownload
            | Action::ScreenConvert
            | Action::ScreenLibrary
            | Action::ScreenSettings
            | Action::Quit => "ekranlar",
            Action::OpenDir | Action::UpdateYtdlp | Action::Help | Action::ToggleFx => "genel",
        }
    }

    pub fn scope(self) -> Scope {
        match self {
            Action::Favorite
            | Action::OnlyFavorites
            | Action::Enqueue
            | Action::PlayNextInQueue
            | Action::RevealPlaying => Scope::Library,
            Action::Quit => Scope::MainMenu,
            Action::UpdateYtdlp => Scope::DownloaderOrSettings,
            _ => Scope::Global,
        }
    }

    fn defaults(self) -> &'static [&'static str] {
        match self {
            Action::PlayPause => &["k:Space", "c:p"],
            Action::Next => &["c:n"],
            Action::Prev => &["c:b"],
            Action::SeekBack => &["k:ArrowLeft"],
            Action::SeekFwd => &["k:ArrowRight"],
            Action::SeekBackLong => &["k:shift+ArrowLeft"],
            Action::SeekFwdLong => &["k:shift+ArrowRight"],
            Action::VolUp => &["c:+", "c:="],
            Action::VolDown => &["c:-"],
            Action::Mute => &["c:m"],
            Action::Stop => &["c:x"],
            Action::Shuffle => &["c:s"],
            Action::Loop => &["c:d", "c:l"],
            Action::Sleep => &["c:t"],
            Action::Search => &["c:/"],
            Action::Favorite => &["c:f"],
            Action::OnlyFavorites => &["c:F"],
            Action::Enqueue => &["c:a"],
            Action::PlayNextInQueue => &["c:A"],
            Action::RevealPlaying => &["c:c"],
            Action::Refresh => &["c:r"],
            Action::OpenDir => &["c:o"],
            Action::UpdateYtdlp => &["c:u"],
            Action::ScreenDownload => &["c:1"],
            Action::ScreenConvert => &["c:2"],
            Action::ScreenLibrary => &["c:3"],
            Action::ScreenSettings => &["c:4"],
            Action::Quit => &["c:q", "c:5"],
            Action::Help => &["c:?"],
            Action::ToggleFx => &["c:v"],
        }
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Binding {
    Char(char),
    Key { key: Key, ctrl: bool, alt: bool, shift: bool },
}

impl Binding {
    pub fn parse(s: &str) -> Option<Binding> {
        if let Some(c) = s.strip_prefix("c:") {
            let mut it = c.chars();
            let ch = it.next()?;
            if it.next().is_some() || ch.is_whitespace() || ch.is_control() {
                return None;
            }
            return Some(Binding::Char(ch));
        }
        let rest = s.strip_prefix("k:")?;
        let (mut ctrl, mut alt, mut shift) = (false, false, false);
        let mut key = None;
        for part in rest.split('+') {
            match part {
                "ctrl" => ctrl = true,
                "alt" => alt = true,
                "shift" => shift = true,
                name => key = Key::from_name(name),
            }
        }
        let key = key?;
        if is_reserved(key) {
            return None;
        }
        Some(Binding::Key { key, ctrl, alt, shift })
    }

    pub fn encode(&self) -> String {
        match self {
            Binding::Char(c) => format!("c:{c}"),
            Binding::Key { key, ctrl, alt, shift } => {
                let mut s = String::from("k:");
                if *ctrl {
                    s.push_str("ctrl+");
                }
                if *alt {
                    s.push_str("alt+");
                }
                if *shift {
                    s.push_str("shift+");
                }
                s.push_str(key.name());
                s
            }
        }
    }

    pub fn label(&self) -> String {
        match self {
            Binding::Char(c) => c.to_string(),
            Binding::Key { key, ctrl, alt, shift } => {
                let mut s = String::new();
                if *ctrl {
                    s.push_str("Ctrl+");
                }
                if *alt {
                    s.push_str("Alt+");
                }
                if *shift {
                    s.push_str("Shift+");
                }
                s.push_str(match key {
                    Key::ArrowLeft => "←",
                    Key::ArrowRight => "→",
                    Key::ArrowUp => "↑",
                    Key::ArrowDown => "↓",
                    Key::Space => "Space",
                    other => other.name(),
                });
                s
            }
        }
    }
}

/// Gezinme için sabit tutulan, atanamayan tuşlar.
pub fn is_reserved(key: Key) -> bool {
    matches!(
        key,
        Key::Escape | Key::Enter | Key::Tab | Key::ArrowUp | Key::ArrowDown | Key::PageUp | Key::PageDown | Key::Home | Key::End
    )
}

/// Yazı üretmeyen (bu yüzden metin olayı gelmeyen) tuşlar.
pub fn is_non_text(key: Key) -> bool {
    use Key::*;
    matches!(
        key,
        Space | ArrowLeft | ArrowRight | Backspace | Insert | Delete
            | F1 | F2 | F3 | F4 | F5 | F6 | F7 | F8 | F9 | F10 | F11 | F12
            | F13 | F14 | F15 | F16 | F17 | F18 | F19 | F20
    )
}

/// Kayıtlı kısayol biçiminin sürümü. 2: ana menüden "yt-dlp guncelle" kalktı,
/// ayarlar 5 → 4, çıkış 6 → 5 oldu.
pub const SHORTCUTS_VERSION: u32 = 2;

/// Eski sürümde kaydedilmiş kısayolları günceller. Sadece eski VARSAYILAN tuşlara
/// dokunur; kullanıcının kendi atadığı tuşlar olduğu gibi kalır.
pub fn migrate_stored(stored: &mut HashMap<String, StoredConf>, from_version: u32) {
    if from_version < 2 {
        stored.remove("screen_update");
        let mut swap = |id: &str, old: &[&str], new: &[&str]| {
            if let Some(c) = stored.get_mut(id)
                && c.keys.iter().map(String::as_str).eq(old.iter().copied())
            {
                c.keys = new.iter().map(|k| (*k).to_owned()).collect();
            }
        };
        swap("screen_settings", &["c:5"], &["c:4"]);
        swap("quit", &["c:q", "c:6"], &["c:q", "c:5"]);
    }
}

#[derive(Clone, Debug)]
pub struct ActionConf {
    pub enabled: bool,
    pub keys: Vec<Binding>,
}

/// Ayar dosyasına yazılan biçim.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct StoredConf {
    pub enabled: bool,
    pub keys: Vec<String>,
}

pub struct Keymap {
    /// Tüm kısayollar için ana anahtar
    pub enabled: bool,
    conf: HashMap<Action, ActionConf>,
}

fn default_conf(a: Action) -> ActionConf {
    ActionConf { enabled: true, keys: a.defaults().iter().filter_map(|s| Binding::parse(s)).collect() }
}

impl Keymap {
    pub fn from_stored(enabled: bool, stored: &HashMap<String, StoredConf>) -> Keymap {
        let mut conf = HashMap::new();
        for a in Action::ALL {
            let c = match stored.get(a.id()) {
                Some(s) => ActionConf {
                    enabled: s.enabled,
                    keys: s.keys.iter().filter_map(|k| Binding::parse(k)).take(SLOTS).collect(),
                },
                None => default_conf(a),
            };
            conf.insert(a, c);
        }
        let mut km = Keymap { enabled, conf };
        km.dedupe();
        km
    }

    /// Elle düzenlenmiş/bozuk ayar dosyasında aynı tuş iki eyleme atanmışsa ilkinde bırakır.
    fn dedupe(&mut self) {
        let mut seen: Vec<Binding> = Vec::new();
        for a in Action::ALL {
            let c = self.conf.get_mut(&a).expect("tum eylemler tanimli");
            c.keys.retain(|b| {
                if seen.contains(b) {
                    false
                } else {
                    seen.push(b.clone());
                    true
                }
            });
        }
    }

    pub fn to_stored(&self) -> HashMap<String, StoredConf> {
        Action::ALL
            .into_iter()
            .map(|a| {
                let c = self.get(a);
                (a.id().to_owned(), StoredConf { enabled: c.enabled, keys: c.keys.iter().map(Binding::encode).collect() })
            })
            .collect()
    }

    pub fn get(&self, a: Action) -> &ActionConf {
        &self.conf[&a]
    }

    pub fn is_default(&self, a: Action) -> bool {
        let d = default_conf(a);
        let c = self.get(a);
        c.enabled == d.enabled && c.keys == d.keys
    }

    pub fn set_enabled(&mut self, a: Action, on: bool) {
        if let Some(c) = self.conf.get_mut(&a) {
            c.enabled = on;
        }
    }

    /// Tuşu eylemin `slot` numaralı yerine atar. Tuş başka bir eylemdeyse oradan alınır
    /// ve o eylem döndürülür (arayüz kullanıcıya bildirir).
    pub fn assign(&mut self, a: Action, slot: usize, b: Binding) -> Option<Action> {
        let mut taken_from = None;
        for (other, c) in self.conf.iter_mut() {
            if *other != a && c.keys.contains(&b) {
                c.keys.retain(|k| *k != b);
                taken_from = Some(*other);
            }
        }
        let c = self.conf.get_mut(&a).expect("tum eylemler tanimli");
        c.keys.retain(|k| *k != b);
        if slot < c.keys.len() {
            c.keys[slot] = b;
        } else if c.keys.len() < SLOTS {
            c.keys.push(b);
        } else {
            c.keys[SLOTS - 1] = b;
        }
        taken_from
    }

    pub fn clear(&mut self, a: Action, slot: usize) {
        if let Some(c) = self.conf.get_mut(&a)
            && slot < c.keys.len()
        {
            c.keys.remove(slot);
        }
    }

    pub fn reset(&mut self, a: Action) {
        let d = default_conf(a);
        // Varsayılan tuşlar başka eyleme atanmışsa onlardan geri alınır
        for b in &d.keys {
            for (other, c) in self.conf.iter_mut() {
                if *other != a {
                    c.keys.retain(|k| k != b);
                }
            }
        }
        self.conf.insert(a, d);
    }

    pub fn reset_all(&mut self) {
        for a in Action::ALL {
            self.conf.insert(a, default_conf(a));
        }
        self.enabled = true;
    }

    fn find(&self, pred: impl Fn(&Binding) -> bool) -> Option<Action> {
        if !self.enabled {
            return None;
        }
        Action::ALL
            .into_iter()
            .find(|a| self.conf[a].enabled && self.conf[a].keys.iter().any(&pred))
    }

    pub fn match_char(&self, ch: char) -> Option<Action> {
        self.find(|b| *b == Binding::Char(ch))
    }

    pub fn match_key(&self, key: Key, m: Modifiers) -> Option<Action> {
        self.find(|b| match b {
            Binding::Key { key: k, ctrl, alt, shift } => {
                *k == key && *ctrl == (m.ctrl || m.command) && *alt == m.alt && *shift == m.shift
            }
            Binding::Char(_) => false,
        })
    }

    /// Kısayol yardımı için: etkin eylemler ve tuşları.
    pub fn active_list(&self) -> Vec<(Action, String)> {
        Action::ALL
            .into_iter()
            .filter(|a| self.conf[a].enabled && !self.conf[a].keys.is_empty())
            .map(|a| (a, self.conf[&a].keys.iter().map(Binding::label).collect::<Vec<_>>().join("  ")))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_unique_and_roundtrip() {
        let km = Keymap::from_stored(true, &HashMap::new());
        let mut all = Vec::new();
        for a in Action::ALL {
            for b in &km.get(a).keys {
                assert!(!all.contains(b), "{:?} iki kez atanmis", b);
                all.push(b.clone());
                assert_eq!(Binding::parse(&b.encode()).as_ref(), Some(b));
            }
            assert!(!km.get(a).keys.is_empty(), "{} varsayilan tussuz", a.id());
        }
        let back = Keymap::from_stored(true, &km.to_stored());
        for a in Action::ALL {
            assert_eq!(back.get(a).keys, km.get(a).keys);
        }
    }

    #[test]
    fn assign_moves_key_and_matching() {
        let mut km = Keymap::from_stored(true, &HashMap::new());
        // 'n' sonraki şarkıdan alınıp sessize atanır
        let taken = km.assign(Action::Mute, 0, Binding::Char('n'));
        assert_eq!(taken, Some(Action::Next));
        assert_eq!(km.match_char('n'), Some(Action::Mute));
        assert!(km.get(Action::Next).keys.is_empty());
        // Kapalı eylem eşleşmez, ana anahtar kapalıyken hiçbir şey eşleşmez
        km.set_enabled(Action::Mute, false);
        assert_eq!(km.match_char('n'), None);
        assert_eq!(km.match_char('p'), Some(Action::PlayPause));
        km.enabled = false;
        assert_eq!(km.match_char('p'), None);
        km.reset_all();
        assert_eq!(km.match_char('n'), Some(Action::Next));
        let shift = Modifiers { shift: true, ..Default::default() };
        assert_eq!(km.match_key(Key::ArrowLeft, shift), Some(Action::SeekBackLong));
        assert_eq!(km.match_key(Key::ArrowLeft, Modifiers::default()), Some(Action::SeekBack));
    }

    #[test]
    fn migrates_old_menu_numbers_but_keeps_custom_keys() {
        let conf = |enabled: bool, keys: &[&str]| StoredConf { enabled, keys: keys.iter().map(|k| (*k).to_owned()).collect() };
        let mut old: HashMap<String, StoredConf> = HashMap::new();
        old.insert("screen_update".into(), conf(true, &["c:4"]));
        old.insert("screen_settings".into(), conf(true, &["c:5"]));
        old.insert("quit".into(), conf(false, &["c:q", "c:6"]));
        old.insert("next".into(), conf(true, &["c:k"]));
        migrate_stored(&mut old, 1);
        let km = Keymap::from_stored(true, &old);
        assert_eq!(km.match_char('4'), Some(Action::ScreenSettings));
        // Çıkış kullanıcı tarafından kapatılmıştı: kapalı kalır ama tuşu güncellenir
        assert_eq!(km.get(Action::Quit).keys, vec![Binding::Char('q'), Binding::Char('5')]);
        assert!(!km.get(Action::Quit).enabled);
        assert_eq!(km.match_char('k'), Some(Action::Next));

        // Kullanıcı ayarlara kendi tuşunu atadıysa dokunulmaz
        let mut custom: HashMap<String, StoredConf> = HashMap::new();
        custom.insert("screen_settings".into(), conf(true, &["c:z"]));
        migrate_stored(&mut custom, 1);
        assert_eq!(custom["screen_settings"].keys, vec!["c:z".to_owned()]);
    }

    #[test]
    fn reserved_and_garbage_rejected() {
        assert_eq!(Binding::parse("k:Escape"), None);
        assert_eq!(Binding::parse("k:Enter"), None);
        assert_eq!(Binding::parse("c:"), None);
        assert_eq!(Binding::parse("c:ab"), None);
        assert_eq!(Binding::parse("c: "), None);
        assert_eq!(Binding::parse("x:y"), None);
        assert_eq!(Binding::parse("k:ctrl+NoSuchKey"), None);
    }
}
