//! Uygulama durumu ve iş mantığı (arayüzden bağımsız).
//! Eski TUI'deki `App` yapısının GUI için genişletilmiş hali.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

use rand::RngExt;

use crate::config::Config;
use crate::converter;
use crate::downloader;
use crate::keymap::Keymap;
use crate::library;
use crate::mpris::{Mpris, MprisCmd, MprisState};
use crate::player::{AudioPlayer, LoopMode, PlaybackStatus, PlayerSnapshot};
use crate::settings::Settings;
use crate::utils;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Screen {
    MainMenu,
    Downloader,
    Converter,
    Library,
    Report,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SortMode {
    Number,
    NumberDesc,
    Name,
}

impl SortMode {
    pub fn label(self) -> &'static str {
        match self {
            SortMode::Number => "numara ↑",
            SortMode::NumberDesc => "numara ↓",
            SortMode::Name => "isim a-z",
        }
    }
    fn key(self) -> &'static str {
        match self {
            SortMode::Number => "number",
            SortMode::NumberDesc => "number_desc",
            SortMode::Name => "name",
        }
    }
    fn from_key(s: &str) -> Self {
        match s {
            "number_desc" => SortMode::NumberDesc,
            "name" => SortMode::Name,
            _ => SortMode::Number,
        }
    }
}

/// Uyku zamanlayıcısı
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SleepTimer {
    Off,
    At(Instant),
    EndOfTrack,
}

pub const SLEEP_CHOICES_MIN: [u64; 5] = [15, 30, 45, 60, 90];

#[derive(Clone, Debug)]
pub struct Song {
    /// Disk üzerindeki dosya adı (örn. "0001 - Sanatci - Sarki.opus")
    pub file: String,
    /// Ekranda gösterilen ad (numara ve uzantı olmadan)
    pub title: String,
    pub number: Option<u32>,
    lower: String,
}

impl Song {
    fn new(file: String) -> Self {
        let clean = utils::strip_control_chars(&file);
        let number = utils::parse_track_number(&file);
        let stem = clean.strip_suffix(".opus").unwrap_or(&clean).to_string();
        let title = if number.is_some() {
            // "0001 - Ad" → "Ad"
            stem.split_once(" - ").map(|(_, rest)| rest.trim()).unwrap_or("")
        } else {
            stem.trim()
        };
        // İsmi boş dosyalar listede boş satır olarak kalmasın
        let title = if title.is_empty() { "(isimsiz)".to_string() } else { title.to_string() };
        let lower = stem.to_lowercase();
        Self { file, title, number, lower }
    }
}

/// Arka plan teması (Ayarlar & Kısayollar → ayarlar → görünüm).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Theme {
    /// Izgara, veri sütunları, radar, frekans analizörü
    Siber,
    /// Yukarıdan bakılan ASCII balık göleti
    Balik,
}

impl Theme {
    pub const ALL: [Theme; 2] = [Theme::Siber, Theme::Balik];

    pub fn key(self) -> &'static str {
        match self {
            Theme::Siber => "siber",
            Theme::Balik => "balik",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Theme::Siber => "siber",
            Theme::Balik => "balik goleti",
        }
    }

    /// Bilinmeyen/eski değerler varsayılana düşer
    pub fn from_key(s: &str) -> Self {
        Self::ALL.into_iter().find(|t| t.key() == s).unwrap_or(Theme::Siber)
    }
}

pub struct Toast {
    pub text: String,
    pub since: Instant,
    pub until: Instant,
}

pub struct App {
    pub screen: Screen,
    pub music_dir: PathBuf,
    pub log_file: PathBuf,
    pub menu_index: usize,

    // ── İndirici ─────────────────────────────────────────
    pub url_input: String,
    pub url_queue: VecDeque<String>,
    pub is_downloading: bool,
    pub is_updating: bool,
    pub pending_playlist: Option<(String, Vec<String>)>,
    pub playlist_choice_yes: bool,
    pub pending_update_offer: bool,
    pub update_choice_yes: bool,
    songs_before_download: usize,
    pub busy_since: Option<Instant>,

    // ── Dönüştürücü ──────────────────────────────────────
    pub conv_input: String,
    pub conv_output: String,
    pub conv_bitrate: String,
    pub is_converting: bool,

    // ── Log ──────────────────────────────────────────────
    pub logs: Vec<String>,
    /// Eklenen log satırı sayacı (eski satırlar silinse de artar)
    pub log_gen: u64,
    rx: Receiver<String>,
    tx: Sender<String>,

    // ── Kütüphane ────────────────────────────────────────
    pub library_stats: Option<library::LibraryStats>,
    pub songs: Vec<Song>,
    /// Kütüphane her yeniden okunduğunda artar
    pub library_gen: u64,
    index_of: HashMap<String, usize>,
    pub filtered: Vec<usize>,
    pub selected: usize,
    pub scroll_to_selected: bool,
    pub search_query: String,
    pub sort_mode: SortMode,
    pub only_favorites: bool,
    pub favorites: HashSet<String>,

    // ── Telefonla eşitleme ───────────────────────────────
    pub sync: crate::sync::SyncState,

    // ── Çalar ────────────────────────────────────────────
    pub player: AudioPlayer,
    pub snap: PlayerSnapshot,
    pub playing: Option<String>,
    pub shuffle: bool,
    shuffle_bag: Vec<String>,
    pub loop_mode: LoopMode,
    last_finished_seq: u64,
    history: Vec<String>,
    pub recent: VecDeque<String>,
    pub queue: VecDeque<String>,
    pub sleep: SleepTimer,
    pub muted_from: Option<u32>,

    // ── Rapor / Ayarlar ──────────────────────────────────
    pub ytdlp_version: Option<String>,
    pub dir_input: String,
    pub keymap: Keymap,
    pub fx_enabled: bool,
    pub panel_alpha: u8,
    pub theme: Theme,

    pub toast: Option<Toast>,
    pub should_quit: bool,
    /// Medya tuşları / masaüstü medya denetimi (D-Bus MPRIS); yoksa `None`
    pub mpris: Option<Mpris>,
    /// MPRIS "Raise": pencereyi öne getir
    pub raise_requested: bool,
    settings: Settings,
    settings_dirty: bool,
    last_save: Instant,
}

impl App {
    pub fn new(mut config: Config, cli_dir_given: bool) -> Self {
        let settings = Settings::load();
        if !cli_dir_given
            && let Some(dir) = settings.music_dir.as_ref().filter(|d| d.is_dir()) {
            config.music_dir = dir.clone();
            config.log_file = dir.join("downloaded.txt");
        }

        let (tx, rx) = channel();
        let player = AudioPlayer::new();
        let snap = player.snapshot();
        let initial_seq = snap.track_finished_seq;
        player.set_volume(settings.volume);

        let loop_mode = match settings.loop_mode.as_str() {
            "all" => LoopMode::All,
            "one" => LoopMode::One,
            _ => LoopMode::Off,
        };

        let mut app = Self {
            screen: Screen::MainMenu,
            music_dir: config.music_dir.clone(),
            log_file: config.log_file.clone(),
            menu_index: 0,

            url_input: String::new(),
            url_queue: VecDeque::new(),
            is_downloading: false,
            is_updating: false,
            pending_playlist: None,
            playlist_choice_yes: false,
            pending_update_offer: false,
            update_choice_yes: false,
            songs_before_download: 0,
            busy_since: None,

            conv_input: config.music_dir.join("mp3").display().to_string(),
            conv_output: config.music_dir.join("mp3/opus_output").display().to_string(),
            conv_bitrate: if utils::is_valid_bitrate(&settings.bitrate) {
                settings.bitrate.clone()
            } else {
                "128k".to_string()
            },
            is_converting: false,

            logs: Vec::new(),
            log_gen: 0,
            rx,
            tx,

            library_stats: None,
            songs: Vec::new(),
            library_gen: 0,
            index_of: HashMap::new(),
            filtered: Vec::new(),
            selected: 0,
            scroll_to_selected: false,
            search_query: String::new(),
            sort_mode: SortMode::from_key(&settings.sort),
            only_favorites: false,
            favorites: settings.favorites.iter().cloned().collect(),
            sync: Default::default(),

            player,
            snap,
            playing: None,
            shuffle: settings.shuffle,
            shuffle_bag: Vec::new(),
            loop_mode,
            last_finished_seq: initial_seq,
            history: Vec::new(),
            recent: VecDeque::new(),
            queue: VecDeque::new(),
            sleep: SleepTimer::Off,
            muted_from: None,

            ytdlp_version: None,
            dir_input: config.music_dir.display().to_string(),
            keymap: {
                let mut stored = settings.shortcuts.clone();
                crate::keymap::migrate_stored(&mut stored, settings.shortcuts_version);
                Keymap::from_stored(settings.shortcuts_enabled, &stored)
            },
            fx_enabled: settings.fx_enabled,
            panel_alpha: settings.panel_alpha,
            theme: Theme::from_key(&settings.theme),

            toast: None,
            should_quit: false,
            mpris: Mpris::start(),
            raise_requested: false,
            settings,
            settings_dirty: false,
            last_save: Instant::now(),
        };
        app.refresh_library();

        // Son çalınan şarkıyı seçili getir (otomatik çalmaz)
        if let Some(last) = app.settings.last_track.clone()
            && let Some(&idx) = app.index_of.get(&last)
                && let Some(row) = app.filtered.iter().position(|&i| i == idx) {
                    app.selected = row;
                    app.scroll_to_selected = true;
                }
        app.fetch_ytdlp_version();
        app
    }

    // ── Yardımcılar ──────────────────────────────────────

    pub fn notify(&mut self, text: impl Into<String>) {
        self.toast = Some(Toast {
            text: utils::strip_control_chars(&text.into()),
            since: Instant::now(),
            until: Instant::now() + Duration::from_millis(2800),
        });
    }

    pub fn add_log(&mut self, msg: String) {
        self.logs.push(utils::strip_control_chars(&msg));
        self.log_gen += 1;
        if self.logs.len() > 500 {
            let extra = self.logs.len() - 500;
            self.logs.drain(..extra);
        }
    }

    pub fn is_busy(&self) -> bool {
        self.is_downloading || self.is_updating || self.is_converting
    }

    pub fn song_by_file(&self, file: &str) -> Option<&Song> {
        self.index_of.get(file).and_then(|&i| self.songs.get(i))
    }

    pub fn display_name(&self, file: &str) -> String {
        match self.song_by_file(file) {
            Some(s) => match s.number {
                Some(n) => format!("{n:04}  {}", s.title),
                None => s.title.clone(),
            },
            None => utils::strip_control_chars(file.strip_suffix(".opus").unwrap_or(file)),
        }
    }

    pub fn mark_dirty(&mut self) {
        self.settings_dirty = true;
    }

    pub fn save_settings_now(&mut self) {
        self.settings.music_dir = Some(self.music_dir.clone());
        self.settings.volume = self.muted_from.unwrap_or(self.snap.volume_pct);
        self.settings.shuffle = self.shuffle;
        self.settings.loop_mode = match self.loop_mode {
            LoopMode::Off => "off",
            LoopMode::All => "all",
            LoopMode::One => "one",
        }
        .to_string();
        let mut favs: Vec<String> = self.favorites.iter().cloned().collect();
        favs.sort();
        self.settings.favorites = favs;
        if let Some(p) = &self.playing {
            self.settings.last_track = Some(p.clone());
        }
        self.settings.bitrate = self.conv_bitrate.clone();
        self.settings.sort = self.sort_mode.key().to_string();
        self.settings.shortcuts_enabled = self.keymap.enabled;
        self.settings.shortcuts = self.keymap.to_stored();
        self.settings.shortcuts_version = crate::keymap::SHORTCUTS_VERSION;
        self.settings.fx_enabled = self.fx_enabled;
        self.settings.panel_alpha = self.panel_alpha;
        self.settings.theme = self.theme.key().to_string();
        self.settings.save();
        self.settings_dirty = false;
        self.last_save = Instant::now();
    }

    // ── Kütüphane ────────────────────────────────────────

    pub fn refresh_library(&mut self) {
        self.library_gen += 1;
        if let Ok(stats) = library::inspect_library(&self.music_dir, &self.log_file) {
            self.library_stats = Some(stats);
        } else {
            self.library_stats = None;
        }
        let mut files = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&self.music_dir) {
            for entry in entries.flatten() {
                let is_file = entry.file_type().map(|t| t.is_file()).unwrap_or(false);
                let name = entry.file_name().to_string_lossy().to_string();
                if is_file && name.ends_with(".opus") {
                    files.push(name);
                }
            }
        }
        files.sort();
        self.songs = files.into_iter().map(Song::new).collect();
        self.index_of = self
            .songs
            .iter()
            .enumerate()
            .map(|(i, s)| (s.file.clone(), i))
            .collect();
        // Silinmiş dosyaları kuyruk / geçmişten temizle
        let idx = &self.index_of;
        self.queue.retain(|f| idx.contains_key(f));
        self.recent.retain(|f| idx.contains_key(f));
        self.history.retain(|f| idx.contains_key(f));
        self.shuffle_bag.clear();
        self.rebuild_filter();
    }

    pub fn rebuild_filter(&mut self) {
        let selected_file = self
            .filtered
            .get(self.selected)
            .and_then(|&i| self.songs.get(i))
            .map(|s| s.file.clone());

        let q = self.search_query.trim().to_lowercase();
        let terms: Vec<&str> = q.split_whitespace().collect();
        let mut list: Vec<usize> = self
            .songs
            .iter()
            .enumerate()
            .filter(|(_, s)| !self.only_favorites || self.favorites.contains(&s.file))
            .filter(|(_, s)| terms.iter().all(|t| s.lower.contains(t)))
            .map(|(i, _)| i)
            .collect();

        match self.sort_mode {
            SortMode::Number => {} // dosya adına göre zaten sıralı
            SortMode::NumberDesc => list.reverse(),
            SortMode::Name => list.sort_by(|&a, &b| {
                self.songs[a]
                    .title
                    .to_lowercase()
                    .cmp(&self.songs[b].title.to_lowercase())
            }),
        }
        self.filtered = list;

        // Seçimi aynı şarkıda tutmaya çalış
        let keep = selected_file
            .and_then(|f| self.index_of.get(&f).copied())
            .and_then(|idx| self.filtered.iter().position(|&i| i == idx));
        self.selected = keep.unwrap_or_default();
        self.scroll_to_selected = true;
    }

    pub fn set_sort(&mut self, mode: SortMode) {
        if self.sort_mode != mode {
            self.sort_mode = mode;
            self.rebuild_filter();
            self.mark_dirty();
        }
    }

    pub fn toggle_only_favorites(&mut self) {
        self.only_favorites = !self.only_favorites;
        self.rebuild_filter();
    }

    pub fn move_selection(&mut self, delta: isize) {
        if self.filtered.is_empty() {
            self.selected = 0;
            return;
        }
        let max = self.filtered.len() as isize - 1;
        self.selected = (self.selected as isize + delta).clamp(0, max) as usize;
        self.scroll_to_selected = true;
    }

    pub fn select_last(&mut self) {
        self.selected = self.filtered.len().saturating_sub(1);
        self.scroll_to_selected = true;
    }

    pub fn selected_file(&self) -> Option<String> {
        self.filtered
            .get(self.selected)
            .and_then(|&i| self.songs.get(i))
            .map(|s| s.file.clone())
    }

    pub fn reveal_playing(&mut self) {
        if let Some(p) = self.playing.clone()
            && let Some(&idx) = self.index_of.get(&p)
                && let Some(row) = self.filtered.iter().position(|&i| i == idx) {
                    self.selected = row;
                    self.scroll_to_selected = true;
                }
    }

    pub fn is_favorite(&self, file: &str) -> bool {
        self.favorites.contains(file)
    }

    pub fn toggle_favorite(&mut self, file: &str) {
        if !self.favorites.remove(file) {
            self.favorites.insert(file.to_string());
            self.notify("★ favorilere eklendi");
        } else {
            self.notify("☆ favorilerden cikarildi");
        }
        if self.only_favorites {
            self.rebuild_filter();
        }
        self.mark_dirty();
    }

    pub fn open_music_dir(&mut self) {
        // Göreli yol `-` ile başlarsa xdg-open onu seçenek sanmasın
        let dir = std::fs::canonicalize(&self.music_dir).unwrap_or_else(|_| self.music_dir.clone());
        if utils::safe_command("xdg-open").arg(&dir).spawn().is_err() {
            self.notify("xdg-open bulunamadi");
        }
    }

    pub fn change_music_dir(&mut self) {
        let raw = PathBuf::from(self.dir_input.trim());
        // Mutlak, sembolik bağları çözülmüş yol: xdg-open/ffmpeg'e `-` ile başlayan
        // göreli bir yol (seçenek gibi yorumlanabilecek) asla geçmez.
        let path = match std::fs::canonicalize(&raw) {
            Ok(p) if !raw.as_os_str().is_empty() && p.is_dir() => p,
            _ => {
                self.notify("Klasor bulunamadi");
                return;
            }
        };
        self.music_dir = path.clone();
        self.log_file = path.join("downloaded.txt");
        self.conv_input = path.join("mp3").display().to_string();
        self.conv_output = path.join("mp3/opus_output").display().to_string();
        self.refresh_library();
        self.mark_dirty();
        self.notify(format!("Muzik klasoru: {}", path.display()));
    }

    // ── Çalma ────────────────────────────────────────────

    pub fn play_file(&mut self, file: &str, push_history: bool) {
        if !self.index_of.contains_key(file) {
            return;
        }
        if push_history
            && let Some(prev) = self.playing.clone()
                && prev != file {
                    self.history.push(prev);
                    if self.history.len() > 200 {
                        self.history.remove(0);
                    }
                }
        self.recent.retain(|f| f != file);
        self.recent.push_front(file.to_string());
        self.recent.truncate(30);
        self.playing = Some(file.to_string());
        self.shuffle_bag.retain(|f| f != file);
        self.player.play(self.music_dir.join(file));
        self.settings.last_track = Some(file.to_string());
        self.mark_dirty();
    }

    pub fn play_selected(&mut self) {
        if let Some(f) = self.selected_file() {
            self.play_file(&f, true);
        }
    }

    /// Sıralı çalmada kullanılacak liste: görünür (filtrelenmiş) liste, çalan şarkı
    /// filtrede yoksa tüm kütüphane.
    fn play_context(&self) -> Vec<usize> {
        let in_view = self
            .playing
            .as_ref()
            .and_then(|p| self.index_of.get(p))
            .is_some_and(|idx| self.filtered.contains(idx));
        if in_view || (self.playing.is_none() && !self.filtered.is_empty()) {
            self.filtered.clone()
        } else {
            (0..self.songs.len()).collect()
        }
    }

    fn next_shuffled(&mut self, ctx: &[usize]) -> Option<String> {
        if ctx.is_empty() {
            return None;
        }
        if self.shuffle_bag.is_empty() {
            let mut bag: Vec<String> = ctx.iter().map(|&i| self.songs[i].file.clone()).collect();
            let mut rng = rand::rng();
            // Fisher–Yates karıştırma: tüm şarkılar bitmeden tekrar yok
            for i in (1..bag.len()).rev() {
                let j = rng.random_range(0..=i);
                bag.swap(i, j);
            }
            if bag.len() > 1 && bag.last() == self.playing.as_ref() {
                let last = bag.len() - 1;
                bag.swap(0, last);
            }
            self.shuffle_bag = bag;
        }
        self.shuffle_bag.pop()
    }

    pub fn play_next(&mut self, manual: bool) {
        if self.songs.is_empty() {
            return;
        }
        if !manual && self.loop_mode == LoopMode::One
            && let Some(cur) = self.playing.clone() {
            self.play_file(&cur, false);
            return;
        }
        // Kuyruk her zaman önceliklidir
        while let Some(next) = self.queue.pop_front() {
            if self.index_of.contains_key(&next) {
                self.play_file(&next, true);
                return;
            }
        }

        let ctx = self.play_context();
        if ctx.is_empty() {
            return;
        }
        if self.shuffle {
            if let Some(f) = self.next_shuffled(&ctx) {
                self.play_file(&f, true);
            }
            return;
        }

        let cur_pos = self
            .playing
            .as_ref()
            .and_then(|p| self.index_of.get(p))
            .and_then(|idx| ctx.iter().position(|i| i == idx));
        let target = match cur_pos {
            Some(p) if p + 1 < ctx.len() => Some(ctx[p + 1]),
            Some(_) => {
                if manual || self.loop_mode == LoopMode::All {
                    Some(ctx[0])
                } else {
                    None
                }
            }
            None => Some(ctx[0]),
        };
        match target {
            Some(i) => {
                let f = self.songs[i].file.clone();
                self.play_file(&f, true);
            }
            None => {
                self.playing = None;
                self.notify("Liste bitti");
            }
        }
    }

    pub fn play_prev(&mut self) {
        if self.songs.is_empty() {
            return;
        }
        // 3 saniyeden fazla çaldıysa önce başa sar (klasik oynatıcı davranışı)
        if self.playing.is_some() && self.snap.position_ms > 3000 {
            self.player.seek_to(0);
            return;
        }
        while let Some(prev) = self.history.pop() {
            if self.index_of.contains_key(&prev) {
                self.play_file(&prev, false);
                return;
            }
        }
        let ctx = self.play_context();
        if ctx.is_empty() {
            return;
        }
        let cur_pos = self
            .playing
            .as_ref()
            .and_then(|p| self.index_of.get(p))
            .and_then(|idx| ctx.iter().position(|i| i == idx));
        let target = match cur_pos {
            Some(0) | None => ctx[ctx.len() - 1],
            Some(p) => ctx[p - 1],
        };
        let f = self.songs[target].file.clone();
        self.play_file(&f, false);
    }

    pub fn toggle_pause(&mut self) {
        if self.snap.status == PlaybackStatus::Idle {
            // Hiçbir şey çalmıyorsa seçili şarkıyı başlat
            if self.playing.is_none() || self.snap.track_name.is_none() {
                if self.selected_file().is_some() {
                    self.play_selected();
                } else {
                    self.play_next(true);
                }
                return;
            }
        }
        self.player.toggle_pause();
    }

    pub fn stop(&mut self) {
        self.player.stop();
        self.playing = None;
    }

    pub fn toggle_shuffle(&mut self) {
        self.shuffle = !self.shuffle;
        self.shuffle_bag.clear();
        self.mark_dirty();
        self.notify(if self.shuffle { "karistir: acik" } else { "karistir: kapali" });
    }

    pub fn cycle_loop(&mut self) {
        self.loop_mode = self.loop_mode.next();
        self.mark_dirty();
        self.notify(format!("dongu: {}", self.loop_mode.label()));
    }

    pub fn set_volume(&mut self, pct: u32) {
        self.muted_from = None;
        self.player.set_volume(pct);
        self.snap.volume_pct = pct.min(150);
        self.mark_dirty();
    }

    pub fn adjust_volume(&mut self, delta: i32) {
        let cur = self.muted_from.unwrap_or(self.snap.volume_pct) as i32;
        self.set_volume((cur + delta).clamp(0, 150) as u32);
    }

    pub fn toggle_mute(&mut self) {
        match self.muted_from.take() {
            Some(v) => {
                self.player.set_volume(v);
                self.snap.volume_pct = v;
            }
            None => {
                self.muted_from = Some(self.snap.volume_pct.max(1));
                self.player.set_volume(0);
                self.snap.volume_pct = 0;
            }
        }
    }

    pub fn enqueue(&mut self, file: &str, front: bool) {
        if !self.index_of.contains_key(file) {
            return;
        }
        if front {
            self.queue.push_front(file.to_string());
            self.notify("Siradaki olarak eklendi");
        } else {
            self.queue.push_back(file.to_string());
            self.notify(format!("Siraya eklendi ({})", self.queue.len()));
        }
    }

    pub fn queue_move(&mut self, idx: usize, up: bool) {
        if up && idx > 0 && idx < self.queue.len() {
            self.queue.swap(idx, idx - 1);
        } else if !up && idx + 1 < self.queue.len() {
            self.queue.swap(idx, idx + 1);
        }
    }

    pub fn cycle_sleep(&mut self) {
        // Kapali → 15 → 30 → 45 → 60 → 90 dk → şarkı sonu → Kapali
        let now = Instant::now();
        self.sleep = match self.sleep {
            SleepTimer::Off => SleepTimer::At(now + Duration::from_secs(SLEEP_CHOICES_MIN[0] * 60)),
            SleepTimer::At(t) => {
                let remaining = t.saturating_duration_since(now).as_secs() / 60 + 1;
                match SLEEP_CHOICES_MIN.iter().find(|&&m| m > remaining) {
                    Some(&m) => SleepTimer::At(now + Duration::from_secs(m * 60)),
                    None => SleepTimer::EndOfTrack,
                }
            }
            SleepTimer::EndOfTrack => SleepTimer::Off,
        };
        let msg = self.sleep_label();
        self.notify(format!("uyku: {msg}"));
    }

    pub fn sleep_label(&self) -> String {
        match self.sleep {
            SleepTimer::Off => "kapali".to_string(),
            SleepTimer::EndOfTrack => "sarki sonu".to_string(),
            SleepTimer::At(t) => {
                let s = t.saturating_duration_since(Instant::now()).as_secs();
                format!("{:02}:{:02}", s / 60, s % 60)
            }
        }
    }

    // ── İndirme ──────────────────────────────────────────

    /// URL alanındaki (boşlukla ayrılmış) linkleri kuyruğa alır ve başlatır.
    pub fn submit_urls(&mut self) {
        let text = std::mem::take(&mut self.url_input);
        let mut added = 0;
        for part in text.split_whitespace() {
            if part.chars().count() > utils::MAX_URL_LEN {
                self.add_log("Cok uzun URL atlandi.".to_string());
                continue;
            }
            if self.url_queue.len() >= 200 {
                break;
            }
            self.url_queue.push_back(part.to_string());
            added += 1;
        }
        if added > 1 {
            self.add_log(format!("{added} link siraya alindi."));
        }
        self.pump_download_queue();
    }

    fn pump_download_queue(&mut self) {
        // Eşitleme numara dağıtırken indirici bekler (aynı numara iki şarkıya verilmesin)
        if self.is_downloading || self.is_updating || self.pending_playlist.is_some() || self.sync.busy() {
            return;
        }
        while let Some(url) = self.url_queue.pop_front() {
            self.pending_update_offer = false;
            let Some((processed_url, is_pl)) = utils::normalize_url(&url) else {
                self.add_log(format!(
                    "Guvenlik: reddedildi ({}) — sadece https://youtube.com, youtu.be, music.youtube.com",
                    utils::strip_control_chars(&url)
                ));
                continue;
            };
            if is_pl {
                let single_ids = utils::extract_video_id(&url).into_iter().collect();
                self.pending_playlist = Some((processed_url, single_ids));
                self.playlist_choice_yes = false;
                return;
            }
            self.trigger_download_execution(url, false);
            return;
        }
    }

    pub fn confirm_playlist_choice(&mut self, download_all: bool) {
        if let Some((pl_url, single_ids)) = self.pending_playlist.take() {
            if download_all {
                self.trigger_download_execution(pl_url, true);
            } else if let Some(id) = single_ids.first() {
                let single_url = format!("https://www.youtube.com/watch?v={id}");
                self.add_log("Playlist reddedildi, sadece tekli video indiriliyor...".to_string());
                self.trigger_download_execution(single_url, false);
            } else {
                self.add_log("Playlist reddedildi ve tekil video kimligi bulunamadi.".to_string());
                self.pump_download_queue();
            }
        }
    }

    pub fn cancel_playlist(&mut self) {
        if self.pending_playlist.take().is_some() {
            self.add_log("Islem iptal edildi.".to_string());
            self.pump_download_queue();
        }
    }

    fn trigger_download_execution(&mut self, url: String, is_playlist_confirmed: bool) {
        self.is_downloading = true;
        self.busy_since = Some(Instant::now());
        self.pending_update_offer = false;
        self.songs_before_download = self.songs.len();
        let out_dir = self.music_dir.clone();
        let log_file = self.log_file.clone();
        let tx = self.tx.clone();

        thread::spawn(move || {
            let Some((processed_url, is_pl)) = utils::normalize_url(&url) else {
                let _ = tx.send("Gecersiz veya izin verilmeyen URL reddedildi.".to_string());
                let _ = tx.send("__DONE__".to_string());
                return;
            };
            let _ = tx.send(format!("URL: {processed_url}"));

            let ids = if is_pl && is_playlist_confirmed {
                let _ = tx.send("Playlist taraniyor...".to_string());
                match downloader::extract_playlist_ids(&processed_url) {
                    Ok(ids) => ids,
                    Err(e) => {
                        let _ = tx.send(format!("Hata: {e}"));
                        let _ = tx.send("__DOWNLOAD_HAD_ERROR__".to_string());
                        let _ = tx.send("__DONE__".to_string());
                        return;
                    }
                }
            } else if let Some(vid) = utils::extract_video_id(&url) {
                vec![vid]
            } else {
                let _ = tx.send("Gecerli bir YouTube ID bulunamadi.".to_string());
                let _ = tx.send("__DONE__".to_string());
                return;
            };

            let next_num = library::get_next_number(&out_dir);
            let _ = tx.send(format!("Siradaki numara: {next_num:04}"));
            match downloader::process_download_queue(ids, &out_dir, &log_file, next_num, Some(&tx)) {
                Ok(report) => {
                    if report.failed > 0 {
                        let _ = tx.send("__DOWNLOAD_HAD_ERROR__".to_string());
                    }
                }
                Err(e) => {
                    let _ = tx.send(format!("Hata: {e}"));
                    let _ = tx.send("__DOWNLOAD_HAD_ERROR__".to_string());
                }
            }
            let _ = tx.send("__DONE__".to_string());
        });
    }

    pub fn answer_update_offer(&mut self, yes: bool) {
        self.pending_update_offer = false;
        if yes {
            self.start_ytdlp_update();
        } else {
            self.add_log("yt-dlp guncellemesi atlandi.".to_string());
        }
    }

    pub fn start_ytdlp_update(&mut self) {
        if self.is_downloading || self.is_updating {
            return;
        }
        self.pending_update_offer = false;
        self.is_updating = true;
        self.busy_since = Some(Instant::now());
        let tx = self.tx.clone();
        thread::spawn(move || {
            if let Err(e) = downloader::update_yt_dlp(Some(&tx)) {
                let _ = tx.send(format!("Hata: {e}"));
            }
            let _ = tx.send("__DONE_UPDATE__".to_string());
        });
    }

    pub fn fetch_ytdlp_version(&mut self) {
        let tx = self.tx.clone();
        thread::spawn(move || {
            let v = downloader::yt_dlp_version().unwrap_or_else(|| "bulunamadi".to_string());
            let _ = tx.send(format!("__YTDLP_VER__{v}"));
        });
    }

    // ── Dönüştürme ───────────────────────────────────────

    pub fn start_conversion(&mut self) {
        if self.is_converting {
            return;
        }
        let bitrate = self.conv_bitrate.trim().to_string();
        if !utils::is_valid_bitrate(&bitrate) {
            self.notify("Gecersiz bitrate (6k–510k)");
            return;
        }
        let in_dir = PathBuf::from(self.conv_input.trim());
        let out_dir = PathBuf::from(self.conv_output.trim());
        if !in_dir.is_dir() {
            self.add_log(format!("Kaynak klasor bulunamadi: {}", in_dir.display()));
            self.notify("Kaynak klasor bulunamadi");
            return;
        }
        self.is_converting = true;
        self.busy_since = Some(Instant::now());
        self.mark_dirty();
        let tx = self.tx.clone();
        thread::spawn(move || {
            let _ = tx.send(format!("Donusturme baslatildi ({bitrate})..."));
            if let Err(e) = converter::batch_convert(&in_dir, &out_dir, &bitrate, Some(&tx)) {
                let _ = tx.send(format!("Hata: {e}"));
            }
            let _ = tx.send("__DONE_CONVERT__".to_string());
        });
    }

    // ── Her karede çağrılır ──────────────────────────────

    /// Eşitleme bittiğinde: kayan dosyaların favorisi/çalan şarkısı yeni ada taşınır, kütüphane yenilenir.
    fn apply_sync_outcome(&mut self, o: crate::sync::client::Outcome) {
        for (old, new) in &o.renamed {
            if self.favorites.remove(old) {
                self.favorites.insert(new.clone());
            }
            if self.playing.as_deref() == Some(old.as_str()) {
                self.playing = Some(new.clone());
            }
        }
        self.mark_dirty();
        self.refresh_library();
        if o.received > 0 {
            self.notify(format!("telefondan {} yeni sarki", o.received));
        }
    }

    pub fn tick(&mut self) {
        let was_syncing = self.sync.busy();
        if let Some(o) = self.sync.poll() {
            self.apply_sync_outcome(o);
        }
        if was_syncing && !self.sync.busy() {
            self.pump_download_queue();
        }
        while let Ok(msg) = self.rx.try_recv() {
            match msg.as_str() {
                "__DONE__" => {
                    self.is_downloading = false;
                    self.refresh_library();
                    let added = self.songs.len().saturating_sub(self.songs_before_download);
                    if added > 0 {
                        self.notify(format!("{added} yeni sarki eklendi"));
                    }
                    self.pump_download_queue();
                }
                "__DONE_UPDATE__" => {
                    self.is_updating = false;
                    self.fetch_ytdlp_version();
                    self.pump_download_queue();
                }
                "__DOWNLOAD_HAD_ERROR__" => {
                    self.pending_update_offer = true;
                    self.update_choice_yes = false;
                }
                "__DONE_CONVERT__" => {
                    self.is_converting = false;
                    self.refresh_library();
                    self.notify("Donusturme bitti");
                }
                _ => {
                    if let Some(v) = msg.strip_prefix("__YTDLP_VER__") {
                        self.ytdlp_version = Some(utils::strip_control_chars(v));
                    } else {
                        self.add_log(msg);
                    }
                }
            }
        }
        if !self.is_busy() {
            self.busy_since = None;
        }

        let mut snap = self.player.snapshot();
        if self.muted_from.is_some() {
            snap.volume_pct = 0;
        }
        self.snap = snap;

        if self.snap.track_finished_seq != self.last_finished_seq {
            self.last_finished_seq = self.snap.track_finished_seq;
            if self.sleep == SleepTimer::EndOfTrack {
                self.sleep = SleepTimer::Off;
                self.playing = None;
                self.notify("Uyku zamanlayicisi: durduruldu");
            } else {
                self.play_next(false);
            }
        }

        if let SleepTimer::At(t) = self.sleep
            && Instant::now() >= t {
            self.sleep = SleepTimer::Off;
            if self.snap.status == PlaybackStatus::Playing {
                self.player.toggle_pause();
            }
            self.notify("Uyku zamanlayicisi: duraklatildi");
        }

        if let Some(t) = &self.toast
            && Instant::now() >= t.until {
            self.toast = None;
        }

        if self.settings_dirty && self.last_save.elapsed() > Duration::from_secs(2) {
            self.save_settings_now();
        }

        self.sync_mpris();
    }

    /// Medya tuşlarından/masaüstünden gelen komutları uygular ve durumu geri bildirir.
    fn sync_mpris(&mut self) {
        let Some(cmds) = self.mpris.as_ref().map(Mpris::poll) else {
            return;
        };
        for cmd in cmds {
            match cmd {
                MprisCmd::PlayPause => self.toggle_pause(),
                MprisCmd::Play => {
                    if self.snap.status != PlaybackStatus::Playing {
                        self.toggle_pause();
                    }
                }
                MprisCmd::Pause => {
                    if self.snap.status == PlaybackStatus::Playing {
                        self.player.toggle_pause();
                    }
                }
                MprisCmd::Stop => self.stop(),
                MprisCmd::Next => self.play_next(true),
                MprisCmd::Prev => self.play_prev(),
                MprisCmd::Seek(us) => self.player.seek_relative(us / 1000),
                MprisCmd::SetPosition(us) => {
                    let delta = us / 1000 - self.snap.position_ms as i64;
                    self.player.seek_relative(delta);
                }
                MprisCmd::SetVolume(v) => self.set_volume((v * 100.0).round() as u32),
                MprisCmd::Raise => self.raise_requested = true,
                MprisCmd::Quit => self.should_quit = true,
            }
        }

        let track_index = self.playing.as_ref().and_then(|p| self.index_of.get(p).copied());
        let state = MprisState {
            status: match self.snap.status {
                PlaybackStatus::Playing => "Playing",
                PlaybackStatus::Paused => "Paused",
                PlaybackStatus::Idle => "Stopped",
            },
            track_index,
            title: self.playing.as_deref().map(|f| self.display_name(f)).unwrap_or_default(),
            path: self.playing.as_ref().map(|f| self.music_dir.join(f).display().to_string()),
            length_us: self.snap.duration_ms as i64 * 1000,
            position_us: self.snap.position_ms as i64 * 1000,
            volume: f64::from(self.snap.volume_pct.min(100)) / 100.0,
            shuffle: self.shuffle,
            loop_status: match self.loop_mode {
                LoopMode::Off => "None",
                LoopMode::All => "Playlist",
                LoopMode::One => "Track",
            },
        };
        if let Some(m) = self.mpris.as_mut() {
            m.update(state);
        }
    }
}

impl Drop for App {
    fn drop(&mut self) {
        self.save_settings_now();
        self.player.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::Song;

    #[test]
    fn song_titles_never_empty() {
        let s = Song::new("0023 - さよならクロステック.opus".into());
        assert_eq!((s.number, s.title.as_str()), (Some(23), "さよならクロステック"));
        let s = Song::new("0373 - .opus".into());
        assert_eq!((s.number, s.title.as_str()), (Some(373), "(isimsiz)"));
        let s = Song::new("0001 - A - B.opus".into());
        assert_eq!(s.title, "A - B");
        let s = Song::new("serbest isim.opus".into());
        assert_eq!((s.number, s.title.as_str()), (None, "serbest isim"));
        let s = Song::new(".opus".into());
        assert_eq!(s.title, "(isimsiz)");
    }
}
