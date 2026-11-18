//! MPRIS2 (D-Bus) desteği.
//!
//! Masaüstü ortamı muzikit'i bir medya oynatıcı olarak tanır: klavyedeki medya tuşları
//! (oynat/duraklat, sonraki, önceki), KDE/GNOME medya denetimi, kilit ekranı ve
//! `playerctl` gibi araçlar çalan şarkıyı görür ve yönetebilir.
//!
//! D-Bus'tan gelen komutlar bir kanala yazılır; uygulama her karede bunları işler ve
//! güncel durumu `update` ile geri bildirir (değişen özellikler için sinyal yayılır).
//! Oturum veri yolu yoksa ya da ad zaten alınmışsa (ikinci kopya) sessizce devre dışı kalır.

use std::collections::HashMap;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use zbus::blocking::Connection;
use zbus::object_server::SignalEmitter;
use zbus::zvariant::{ObjectPath, OwnedValue, Value};

const BUS_NAME: &str = "org.mpris.MediaPlayer2.muzikit";
const OBJ_PATH: &str = "/org/mpris/MediaPlayer2";
const NO_TRACK: &str = "/org/mpris/MediaPlayer2/TrackList/NoTrack";

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MprisCmd {
    PlayPause,
    Play,
    Pause,
    Stop,
    Next,
    Prev,
    /// Göreli sarma (mikrosaniye)
    Seek(i64),
    /// Mutlak konum (mikrosaniye)
    SetPosition(i64),
    /// 0.0 – 1.0
    SetVolume(f64),
    Raise,
    Quit,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct MprisState {
    /// "Playing" | "Paused" | "Stopped"
    pub status: &'static str,
    pub track_index: Option<usize>,
    pub title: String,
    pub path: Option<String>,
    pub length_us: i64,
    pub position_us: i64,
    pub volume: f64,
    pub shuffle: bool,
    /// "None" | "Playlist" | "Track"
    pub loop_status: &'static str,
}

type Waker = Arc<Mutex<Option<Box<dyn Fn() + Send>>>>;

#[derive(Clone)]
struct Shared {
    tx: Sender<MprisCmd>,
    state: Arc<Mutex<MprisState>>,
    waker: Waker,
}

impl Shared {
    fn send(&self, cmd: MprisCmd) {
        let _ = self.tx.send(cmd);
        // Pencere küçültülmüş/arkadaysa arayüz seyrek çizilir; komut beklemesin diye uyandır
        if let Ok(w) = self.waker.lock()
            && let Some(f) = w.as_ref()
        {
            f();
        }
    }

    fn state(&self) -> MprisState {
        self.state.lock().map(|s| s.clone()).unwrap_or_default()
    }
}

struct RootIface {
    shared: Shared,
}

#[zbus::interface(name = "org.mpris.MediaPlayer2")]
impl RootIface {
    fn raise(&self) {
        self.shared.send(MprisCmd::Raise);
    }

    fn quit(&self) {
        self.shared.send(MprisCmd::Quit);
    }

    #[zbus(property)]
    fn can_quit(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_raise(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn has_track_list(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn identity(&self) -> &str {
        "muzikit"
    }

    #[zbus(property)]
    fn desktop_entry(&self) -> &str {
        "muzikit"
    }

    #[zbus(property)]
    fn supported_uri_schemes(&self) -> Vec<String> {
        Vec::new()
    }

    #[zbus(property)]
    fn supported_mime_types(&self) -> Vec<String> {
        Vec::new()
    }
}

struct PlayerIface {
    shared: Shared,
}

fn track_path(index: Option<usize>) -> ObjectPath<'static> {
    match index {
        Some(i) => ObjectPath::try_from(format!("/org/muzikit/track/{i}")),
        None => ObjectPath::try_from(NO_TRACK.to_owned()),
    }
    .unwrap_or_else(|_| ObjectPath::from_static_str_unchecked(NO_TRACK))
}

fn owned(v: Value<'_>) -> Option<OwnedValue> {
    OwnedValue::try_from(v).ok()
}

/// `file://` adresi: yüzde kodlaması sadece güvenli karakterleri açık bırakır.
fn file_url(path: &str) -> String {
    let mut out = String::from("file://");
    for b in path.bytes() {
        if b.is_ascii_alphanumeric() || b"/-_.~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

#[zbus::interface(name = "org.mpris.MediaPlayer2.Player")]
impl PlayerIface {
    fn next(&self) {
        self.shared.send(MprisCmd::Next);
    }

    fn previous(&self) {
        self.shared.send(MprisCmd::Prev);
    }

    fn pause(&self) {
        self.shared.send(MprisCmd::Pause);
    }

    fn play_pause(&self) {
        self.shared.send(MprisCmd::PlayPause);
    }

    fn stop(&self) {
        self.shared.send(MprisCmd::Stop);
    }

    fn play(&self) {
        self.shared.send(MprisCmd::Play);
    }

    fn seek(&self, offset: i64) {
        self.shared.send(MprisCmd::Seek(offset));
    }

    fn set_position(&self, track_id: ObjectPath<'_>, position: i64) {
        // Spesifikasyon: farklı parçaya ait istek yok sayılır
        if track_id == track_path(self.shared.state().track_index) && position >= 0 {
            self.shared.send(MprisCmd::SetPosition(position));
        }
    }

    fn open_uri(&self, _uri: &str) {}

    #[zbus(signal)]
    async fn seeked(emitter: &SignalEmitter<'_>, position: i64) -> zbus::Result<()>;

    #[zbus(property)]
    fn playback_status(&self) -> String {
        let s = self.shared.state().status;
        if s.is_empty() { "Stopped".into() } else { s.into() }
    }

    #[zbus(property)]
    fn loop_status(&self) -> String {
        let s = self.shared.state().loop_status;
        if s.is_empty() { "None".into() } else { s.into() }
    }

    #[zbus(property)]
    fn rate(&self) -> f64 {
        1.0
    }

    #[zbus(property)]
    fn shuffle(&self) -> bool {
        self.shared.state().shuffle
    }

    #[zbus(property)]
    fn metadata(&self) -> HashMap<String, OwnedValue> {
        let st = self.shared.state();
        let mut m = HashMap::new();
        if let Some(v) = owned(Value::from(track_path(st.track_index))) {
            m.insert("mpris:trackid".to_owned(), v);
        }
        if st.track_index.is_none() {
            return m;
        }
        if let Some(v) = owned(Value::from(st.length_us)) {
            m.insert("mpris:length".to_owned(), v);
        }
        if let Some(v) = owned(Value::from(st.title.clone())) {
            m.insert("xesam:title".to_owned(), v);
        }
        if let Some(v) = owned(Value::from(vec!["muzikit".to_owned()])) {
            m.insert("xesam:artist".to_owned(), v);
        }
        if let Some(p) = &st.path
            && let Some(v) = owned(Value::from(file_url(p)))
        {
            m.insert("xesam:url".to_owned(), v);
        }
        m
    }

    #[zbus(property)]
    fn volume(&self) -> f64 {
        self.shared.state().volume
    }

    #[zbus(property)]
    fn set_volume(&mut self, value: f64) {
        if value.is_finite() {
            self.shared.send(MprisCmd::SetVolume(value.clamp(0.0, 1.0)));
        }
    }

    #[zbus(property(emits_changed_signal = "false"))]
    fn position(&self) -> i64 {
        self.shared.state().position_us
    }

    #[zbus(property)]
    fn minimum_rate(&self) -> f64 {
        1.0
    }

    #[zbus(property)]
    fn maximum_rate(&self) -> f64 {
        1.0
    }

    #[zbus(property)]
    fn can_go_next(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_go_previous(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_play(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_pause(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_seek(&self) -> bool {
        self.shared.state().track_index.is_some()
    }

    #[zbus(property(emits_changed_signal = "const"))]
    fn can_control(&self) -> bool {
        true
    }
}

pub struct Mpris {
    conn: Connection,
    rx: Receiver<MprisCmd>,
    shared: Shared,
    last: MprisState,
    last_at: Instant,
}

impl Mpris {
    /// D-Bus'a bağlanıp MPRIS adını alır; olmazsa `None`.
    pub fn start() -> Option<Mpris> {
        let (tx, rx) = channel();
        let shared = Shared { tx, state: Arc::new(Mutex::new(MprisState::default())), waker: Arc::new(Mutex::new(None)) };
        let conn = zbus::blocking::connection::Builder::session()
            .ok()?
            .name(BUS_NAME)
            .ok()?
            .serve_at(OBJ_PATH, RootIface { shared: shared.clone() })
            .ok()?
            .serve_at(OBJ_PATH, PlayerIface { shared: shared.clone() })
            .ok()?
            .build()
            .ok()?;
        Some(Mpris { conn, rx, shared, last: MprisState::default(), last_at: Instant::now() })
    }

    /// D-Bus komutu geldiğinde çağrılır (arayüzü hemen yeniden çizdirmek için).
    pub fn set_waker(&self, f: Box<dyn Fn() + Send>) {
        if let Ok(mut w) = self.shared.waker.lock() {
            *w = Some(f);
        }
    }

    pub fn poll(&self) -> Vec<MprisCmd> {
        self.rx.try_iter().collect()
    }

    /// Güncel durumu yayınlar; sadece değişen özellikler için sinyal gönderir.
    pub fn update(&mut self, new: MprisState) {
        let old = std::mem::replace(&mut self.last, new.clone());
        let elapsed_us = self.last_at.elapsed().as_micros() as i64;
        self.last_at = Instant::now();
        if let Ok(mut s) = self.shared.state.lock() {
            *s = new.clone();
        }
        if old == new {
            return;
        }
        let Ok(iref) = self.conn.object_server().interface::<_, PlayerIface>(OBJ_PATH) else {
            return;
        };
        let em = iref.signal_emitter();
        let p = iref.get();
        if old.status != new.status {
            let _ = zbus::block_on(p.playback_status_changed(em));
        }
        if old.track_index != new.track_index || old.title != new.title || old.length_us != new.length_us {
            let _ = zbus::block_on(p.metadata_changed(em));
            let _ = zbus::block_on(p.can_seek_changed(em));
        }
        if (old.volume - new.volume).abs() > f64::EPSILON {
            let _ = zbus::block_on(p.volume_changed(em));
        }
        if old.shuffle != new.shuffle {
            let _ = zbus::block_on(p.shuffle_changed(em));
        }
        if old.loop_status != new.loop_status {
            let _ = zbus::block_on(p.loop_status_changed(em));
        }
        // Normal akıştan sapan konum değişimi = sarma; istemcilerin ilerleme çubuğu için
        let expected = if old.status == "Playing" { old.position_us + elapsed_us } else { old.position_us };
        if old.track_index == new.track_index && (new.position_us - expected).abs() > 1_500_000 {
            let _ = zbus::block_on(PlayerIface::seeked(em, new.position_us));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_url_escapes() {
        assert_eq!(file_url("/a b/ş.opus"), "file:///a%20b/%C5%9F.opus");
        assert_eq!(file_url("/x/0001 - a&b.opus"), "file:///x/0001%20-%20a%26b.opus");
    }

    #[test]
    fn track_paths_valid() {
        assert_eq!(track_path(Some(12)).as_str(), "/org/muzikit/track/12");
        assert_eq!(track_path(None).as_str(), NO_TRACK);
    }
}
