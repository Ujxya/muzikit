use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::future::Future;
use std::pin::Pin;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use byteorder::{LittleEndian, WriteBytesExt};
use pulseaudio::{protocol, AsPlaybackSource, Client, PlaybackStream};
use symphonia::core::codecs::audio::{AudioDecoder, AudioDecoderOptions};
use symphonia::core::codecs::registry::CodecRegistry;
use symphonia::core::codecs::CodecParameters;
use symphonia::core::common::Tier;
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::units::{Time, TimeBase};
use symphonia_opus::OpusDecoder;
use tokio::sync::mpsc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoopMode {
    Off,
    All,
    One,
}

impl LoopMode {
    pub fn next(self) -> Self {
        match self {
            LoopMode::Off => LoopMode::All,
            LoopMode::All => LoopMode::One,
            LoopMode::One => LoopMode::Off,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            LoopMode::Off => "Kapali",
            LoopMode::All => "Liste",
            LoopMode::One => "Tek",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaybackStatus {
    Idle,
    Playing,
    Paused,
}

pub enum PlayerCommand {
    PlayFile(PathBuf),
    TogglePause,
    SeekRelative(i64),
    AdjustVolume(i32),
    Stop,
}

#[derive(Clone, Debug)]
pub struct PlayerSnapshot {
    pub status: PlaybackStatus,
    pub track_name: Option<String>,
    pub track_path: Option<PathBuf>,
    pub position_ms: u64,
    pub duration_ms: u64,
    pub volume_pct: u32,
    pub track_finished_seq: u64,
    pub last_error: Option<String>,
}

impl Default for PlayerSnapshot {
    fn default() -> Self {
        Self {
            status: PlaybackStatus::Idle,
            track_name: None,
            track_path: None,
            position_ms: 0,
            duration_ms: 0,
            volume_pct: 100,
            track_finished_seq: 0,
            last_error: None,
        }
    }
}

/// Analizör için en son sunucuya gönderilen ses örnekleri (mono, ses seviyesinden bağımsız).
pub struct SampleTap {
    inner: Mutex<TapInner>,
}

struct TapInner {
    samples: std::collections::VecDeque<f32>,
    rate: u32,
    last_push: Option<Instant>,
}

impl SampleTap {
    const CAP: usize = 32_768;

    fn new() -> Self {
        Self { inner: Mutex::new(TapInner { samples: Default::default(), rate: 48_000, last_push: None }) }
    }

    fn clear(&self) {
        if let Ok(mut t) = self.inner.lock() {
            t.samples.clear();
            t.last_push = None;
        }
    }

    /// `bytes`: S16LE, `channels` kanallı, `volume_pct` ile ölçeklenmiş PCM.
    fn push(&self, bytes: &[u8], channels: usize, rate: u32, volume_pct: u32) {
        let Ok(mut t) = self.inner.lock() else { return };
        t.rate = rate;
        t.last_push = Some(Instant::now());
        let frame = channels.max(1) * 2;
        // Ses seviyesinin etkisini geri al: çubuklar düğmeden değil müzikten etkilensin
        let gain = if volume_pct == 0 { 0.0 } else { 100.0 / volume_pct as f32 / 32768.0 };
        for f in bytes.chunks_exact(frame) {
            let mut sum = 0.0;
            for c in f.as_chunks::<2>().0 {
                sum += i16::from_le_bytes(*c) as f32;
            }
            t.samples.push_back(sum / channels.max(1) as f32 * gain);
        }
        let excess = t.samples.len().saturating_sub(Self::CAP);
        t.samples.drain(..excess);
    }

    /// Şu an hoparlörden çıkan ana denk gelen `n` örnek (tampon gecikmesi kadar geriden).
    /// Çalma duraklatıldıysa / durduysa `None`.
    pub fn recent(&self, n: usize) -> Option<(Vec<f32>, u32)> {
        let t = self.inner.lock().ok()?;
        let fresh = t.last_push.is_some_and(|p| p.elapsed() < Duration::from_millis(400));
        let lag = (t.rate as f32 * TARGET_LATENCY_SECS) as usize;
        if !fresh || t.samples.len() < n + lag {
            return None;
        }
        let end = t.samples.len() - lag;
        Some((t.samples.range(end - n..end).copied().collect(), t.rate))
    }
}

/// Sunucudan istenen tampon süresi. Varsayılan (~2 sn) yerine kısa tampon: analizör sesle
/// senkron olur, sarma/duraklatma hemen tepki verir, kopmadan kurtarmada atlama küçük kalır.
const TARGET_LATENCY_SECS: f32 = 0.25;

pub struct AudioPlayer {
    cmd_tx: mpsc::UnboundedSender<PlayerCommand>,
    state: Arc<Mutex<PlayerSnapshot>>,
    pos_ms_atomic: Arc<AtomicU64>,
    volume_atomic: Arc<AtomicU32>,
    tap: Arc<SampleTap>,
}

impl AudioPlayer {
    pub fn new() -> Self {
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
        let state = Arc::new(Mutex::new(PlayerSnapshot::default()));
        let pos_ms_atomic = Arc::new(AtomicU64::new(0));
        let volume_atomic = Arc::new(AtomicU32::new(100));

        let state_clone = Arc::clone(&state);
        let pos_clone = Arc::clone(&pos_ms_atomic);
        let vol_clone = Arc::clone(&volume_atomic);
        let tap = Arc::new(SampleTap::new());
        let tap_clone = Arc::clone(&tap);

        thread::Builder::new()
            .name("muzikit-audio".to_string())
            .spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build();
                match rt {
                    Ok(rt) => rt.block_on(audio_loop(cmd_rx, state_clone, pos_clone, vol_clone, tap_clone)),
                    Err(e) => {
                        if let Ok(mut s) = state_clone.lock() {
                            s.last_error = Some(format!("Audio runtime baslatilamadi: {e}"));
                        }
                    }
                }
            })
            .expect("Audio thread baslatilamadi");

        Self {
            cmd_tx,
            state,
            pos_ms_atomic,
            volume_atomic,
            tap,
        }
    }

    /// Frekans analizörü için çalınan ses örnekleri.
    pub fn tap(&self) -> Arc<SampleTap> {
        Arc::clone(&self.tap)
    }

    pub fn play(&self, path: PathBuf) {
        let _ = self.cmd_tx.send(PlayerCommand::PlayFile(path));
    }

    pub fn toggle_pause(&self) {
        let _ = self.cmd_tx.send(PlayerCommand::TogglePause);
    }

    pub fn seek_relative(&self, delta_ms: i64) {
        let _ = self.cmd_tx.send(PlayerCommand::SeekRelative(delta_ms));
    }

    pub fn adjust_volume(&self, delta_pct: i32) {
        let _ = self.cmd_tx.send(PlayerCommand::AdjustVolume(delta_pct));
    }

    pub fn stop(&self) {
        let _ = self.cmd_tx.send(PlayerCommand::Stop);
    }

    /// Şarkı içinde mutlak bir konuma atlar (GUI ilerleme çubuğu için).
    pub fn seek_to(&self, target_ms: u64) {
        let cur = self.pos_ms_atomic.load(Ordering::Relaxed) as i64;
        let delta = target_ms as i64 - cur;
        if delta != 0 {
            self.seek_relative(delta);
        }
    }

    /// Ses seviyesini doğrudan ayarlar (0–150).
    pub fn set_volume(&self, pct: u32) {
        let target = pct.min(150) as i32;
        let cur = self.volume_atomic.load(Ordering::Relaxed) as i32;
        if target != cur {
            self.adjust_volume(target - cur);
        }
    }

    pub fn snapshot(&self) -> PlayerSnapshot {
        let mut snap = self.state.lock().map(|s| s.clone()).unwrap_or_default();
        snap.position_ms = self.pos_ms_atomic.load(Ordering::Relaxed);
        snap.volume_pct = self.volume_atomic.load(Ordering::Relaxed);
        snap
    }
}

struct DecoderState {
    format: Box<dyn FormatReader>,
    decoder: Box<dyn AudioDecoder>,
    track_id: u32,
    time_base: Option<TimeBase>,
    pcm_bytes: Vec<u8>,
    pcm_cursor: usize,
    i16_scratch: Vec<i16>,
    eof: bool,
    duration_ms: u64,
    current_pos_ms: u64,
}

struct OpenedTrack {
    decoder_state: DecoderState,
    sample_rate: u32,
    channels: u8,
    duration_ms: u64,
}

fn build_codec_registry() -> CodecRegistry {
    let mut registry = CodecRegistry::new();
    symphonia::default::register_enabled_codecs(&mut registry);
    registry.register_audio_decoder_at_tier::<OpusDecoder>(Tier::Fallback);
    registry
}

fn open_audio_file(path: &Path) -> Result<OpenedTrack> {
    let file = File::open(path).with_context(|| format!("Dosya acilamadi: {}", path.display()))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }

    let format = symphonia::default::get_probe()
        .probe(
            &hint,
            mss,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .context("Ses dosyasi bicimi taninamadi")?;

    let track = format
        .default_track(TrackType::Audio)
        .ok_or_else(|| anyhow!("Ses izi (audio track) bulunamadi"))?;

    let track_id = track.id;
    let time_base = track.time_base;

    let audio_params = match &track.codec_params {
        Some(CodecParameters::Audio(p)) => p.clone(),
        _ => bail!("Ses kodek parametreleri okunamadi"),
    };

    let sample_rate = audio_params.sample_rate.unwrap_or(48_000);
    let channels = audio_params
        .channels
        .as_ref()
        .map(|c| c.count() as u8)
        .unwrap_or(2)
        .clamp(1, 2);

    let duration_ms = match (time_base, track.duration) {
        (Some(tb), Some(dur)) => tb
            .calc_duration_saturating(dur)
            .as_millis()
            .max(0) as u64,
        _ => 0,
    };

    let registry = build_codec_registry();
    let decoder = registry
        .make_audio_decoder(&audio_params, &AudioDecoderOptions::default())
        .context("Ses cozucu (decoder) baslatilamadi")?;

    Ok(OpenedTrack {
        decoder_state: DecoderState {
            format,
            decoder,
            track_id,
            time_base,
            pcm_bytes: Vec::with_capacity(16384),
            pcm_cursor: 0,
            i16_scratch: Vec::with_capacity(8192),
            eof: false,
            duration_ms,
            current_pos_ms: 0,
        },
        sample_rate,
        channels,
        duration_ms,
    })
}

fn fill_next_packet(state: &mut DecoderState, volume_pct: u32) -> bool {
    state.pcm_bytes.clear();
    state.pcm_cursor = 0;

    let scale = (volume_pct as f32) / 100.0;

    loop {
        let packet = match state.format.next_packet() {
            Ok(Some(pkt)) => pkt,
            Ok(None) => {
                state.eof = true;
                return false;
            }
            Err(SymphoniaError::ResetRequired) => {
                state.decoder.reset();
                continue;
            }
            Err(_) => {
                state.eof = true;
                return false;
            }
        };

        if packet.track_id != state.track_id {
            continue;
        }

        if let Some(tb) = state.time_base {
            let ms = tb.calc_time_saturating(packet.pts).as_millis().max(0) as u64;
            state.current_pos_ms = ms;
        }

        match state.decoder.decode(&packet) {
            Ok(decoded) => {
                if decoded.is_empty() {
                    continue;
                }
                state.i16_scratch.clear();
                decoded.copy_to_vec_interleaved::<i16>(&mut state.i16_scratch);

                if state.i16_scratch.is_empty() {
                    continue;
                }

                state.pcm_bytes.reserve(state.i16_scratch.len() * 2);
                if (scale - 1.0).abs() < f32::EPSILON {
                    for &s in &state.i16_scratch {
                        let _ = state.pcm_bytes.write_i16::<LittleEndian>(s);
                    }
                } else {
                    for &s in &state.i16_scratch {
                        let scaled = ((s as f32) * scale).round().clamp(-32768.0, 32767.0) as i16;
                        let _ = state.pcm_bytes.write_i16::<LittleEndian>(scaled);
                    }
                }
                return true;
            }
            Err(SymphoniaError::DecodeError(_)) => continue,
            Err(_) => {
                state.eof = true;
                return false;
            }
        }
    }
}

fn seek_decoder(state: &mut DecoderState, target_ms: u64) -> Result<u64> {
    let clamped_ms = if state.duration_ms > 0 {
        target_ms.min(state.duration_ms.saturating_sub(250))
    } else {
        target_ms
    };

    let target_time = Time::from_millis_u64(clamped_ms);
    let seeked = state.format.seek(
        SeekMode::Accurate,
        SeekTo::Time {
            time: target_time,
            track_id: Some(state.track_id),
        },
    )?;

    state.decoder.reset();
    state.pcm_bytes.clear();
    state.pcm_cursor = 0;
    state.eof = false;

    let actual_ms = state
        .time_base
        .map(|tb| tb.calc_time_saturating(seeked.actual_ts).as_millis().max(0) as u64)
        .unwrap_or(clamped_ms);
    state.current_pos_ms = actual_ms;

    Ok(actual_ms)
}

/// Çalan akış. `done`, akışın bir kopyasını (Arc) tutar; bu yüzden akış silinmeden
/// ÖNCE düşürülmelidir (bkz. `retire`).
struct ActiveStream {
    stream: PlaybackStream,
    /// Kaynak bitip sunucu tamponu boşaltınca tamamlanır
    done: Pin<Box<dyn Future<Output = Result<(), String>>>>,
    decoder: Arc<Mutex<DecoderState>>,
    stop_flag: Arc<AtomicBool>,
    paused: bool,
    path: PathBuf,
    /// Takılma bekçisi: son görülen konum ve kaç saniyedir ilerlemediği
    last_pos: u64,
    stalled_secs: u32,
    /// Bu akış bir kopmadan sonra kurtarma ile mi açıldı (sonsuz denemeyi önler)
    recovered_at: Option<Instant>,
}

/// Akışı güvenle kapatır.
///
/// `pulseaudio` kütüphanesinde akışın son kopyası düşünce sunucuya kanal numarasıyla
/// "sil" komutu gider. Sunucu silinen akışın kanal numarasını yeni akışa yeniden verir;
/// eski bir kopya yeni akış oluşturulduktan SONRA düşerse yeni akışı siler ve çalma
/// "Ses akisi kesildi" ile durur (şarkı değiştirirken yaşanan hata buydu). Bu yüzden
/// tüm kopyalar burada, yeni akış oluşturulmadan önce düşürülür.
async fn retire(act: ActiveStream) {
    act.stop_flag.store(true, Ordering::Relaxed);
    drop(act.done);
    let _ = act.stream.delete().await;
}

enum LoopEvent {
    Finished(Result<(), String>),
    Cmd(Option<PlayerCommand>),
    Tick,
}

/// Konum bu kadar saniye ilerlemezse akış takılmış sayılır (sunucu akışı sessizce öldürmüş olabilir).
const STALL_SECS: u32 = 3;

async fn audio_loop(
    mut cmd_rx: mpsc::UnboundedReceiver<PlayerCommand>,
    snapshot: Arc<Mutex<PlayerSnapshot>>,
    pos_ms_atomic: Arc<AtomicU64>,
    volume_atomic: Arc<AtomicU32>,
    tap: Arc<SampleTap>,
) {
    let mut client: Option<Client> = None;
    let mut active: Option<ActiveStream> = None;

    let set_idle = |snapshot: &Arc<Mutex<PlayerSnapshot>>, clear_track: bool| {
        if let Ok(mut s) = snapshot.lock() {
            s.status = PlaybackStatus::Idle;
            if clear_track {
                s.track_name = None;
                s.track_path = None;
                s.duration_ms = 0;
            }
        }
    };

    loop {
        let event = match active.as_mut() {
            None => LoopEvent::Cmd(cmd_rx.recv().await),
            Some(act) => {
                tokio::select! {
                    res = &mut act.done => LoopEvent::Finished(res),
                    cmd = cmd_rx.recv() => LoopEvent::Cmd(cmd),
                    _ = tokio::time::sleep(Duration::from_secs(1)) => LoopEvent::Tick,
                }
            }
        };

        // Kopma / takılma: aynı şarkıyı kaldığı yerden yeni bağlantıyla yeniden açmayı dene
        let mut broken: Option<String> = None;

        match event {
            LoopEvent::Finished(res) => {
                let Some(act) = active.take() else { continue };
                if act.stop_flag.load(Ordering::Relaxed) {
                    continue;
                }
                match res {
                    Ok(()) => {
                        // Parça doğal olarak bitti
                        drop(act);
                        pos_ms_atomic.store(0, Ordering::Relaxed);
                        if let Ok(mut s) = snapshot.lock() {
                            s.status = PlaybackStatus::Idle;
                            s.track_finished_seq += 1;
                        }
                    }
                    Err(e) => {
                        active = Some(act);
                        broken = Some(e);
                    }
                }
            }
            LoopEvent::Tick => {
                if let Some(act) = active.as_mut() {
                    let at_eof = act.decoder.lock().map(|d| d.eof).unwrap_or(false);
                    let pos = pos_ms_atomic.load(Ordering::Relaxed);
                    if act.paused || at_eof || pos != act.last_pos {
                        act.last_pos = pos;
                        act.stalled_secs = 0;
                    } else {
                        act.stalled_secs += 1;
                        if act.stalled_secs >= STALL_SECS {
                            broken = Some("akis ilerlemiyor".to_owned());
                        }
                    }
                }
            }
            LoopEvent::Cmd(None) => break,
            LoopEvent::Cmd(Some(cmd)) => match cmd {
                PlayerCommand::TogglePause => {
                    if let Some(act) = active.as_mut() {
                        if act.paused {
                            if act.stream.uncork().await.is_ok() {
                                act.paused = false;
                                act.stalled_secs = 0;
                                if let Ok(mut s) = snapshot.lock() {
                                    s.status = PlaybackStatus::Playing;
                                }
                            }
                        } else if act.stream.cork().await.is_ok() {
                            act.paused = true;
                            if let Ok(mut s) = snapshot.lock() {
                                s.status = PlaybackStatus::Paused;
                            }
                        }
                    }
                }
                PlayerCommand::SeekRelative(delta_ms) => {
                    if let Some(act) = active.as_mut() {
                        let was_paused = act.paused;
                        if !was_paused {
                            let _ = act.stream.cork().await;
                        }
                        let new_pos = act.decoder.lock().ok().and_then(|mut dec| {
                            let target = (dec.current_pos_ms as i64 + delta_ms).max(0) as u64;
                            seek_decoder(&mut dec, target).ok()
                        });
                        let _ = act.stream.flush().await;
                        tap.clear();
                        if let Some(p) = new_pos {
                            pos_ms_atomic.store(p, Ordering::Relaxed);
                            act.last_pos = p;
                        }
                        act.stalled_secs = 0;
                        if !was_paused {
                            let _ = act.stream.uncork().await;
                        }
                    }
                }
                PlayerCommand::AdjustVolume(delta) => {
                    let cur = volume_atomic.load(Ordering::Relaxed) as i32;
                    let next = (cur + delta).clamp(0, 150) as u32;
                    volume_atomic.store(next, Ordering::Relaxed);
                }
                PlayerCommand::Stop => {
                    if let Some(act) = active.take() {
                        retire(act).await;
                    }
                    tap.clear();
                    pos_ms_atomic.store(0, Ordering::Relaxed);
                    set_idle(&snapshot, true);
                }
                PlayerCommand::PlayFile(path) => {
                    if let Some(act) = active.take() {
                        retire(act).await;
                    }
                    active = start_new_stream(&mut client, &path, 0, &snapshot, &pos_ms_atomic, &volume_atomic, &tap)
                        .await
                        .map(|a| ActiveStream { recovered_at: None, ..a });
                }
            },
        }

        if let Some(reason) = broken
            && let Some(act) = active.take()
        {
            let path = act.path.clone();
            let pos = pos_ms_atomic.load(Ordering::Relaxed);
            // Aynı akış kısa süre önce zaten kurtarıldıysa tekrar deneme (sonsuz döngü olmasın)
            let may_retry = act.recovered_at.is_none_or(|t| t.elapsed() > Duration::from_secs(15));
            retire(act).await;
            // Bağlantı kopmuş olabilir: sonraki akış yeni bağlantıyla açılsın
            client = None;
            if may_retry {
                active = start_new_stream(&mut client, &path, pos, &snapshot, &pos_ms_atomic, &volume_atomic, &tap)
                    .await
                    .map(|a| ActiveStream { recovered_at: Some(Instant::now()), ..a });
            }
            if active.is_none() {
                pos_ms_atomic.store(0, Ordering::Relaxed);
                if let Ok(mut s) = snapshot.lock() {
                    s.status = PlaybackStatus::Idle;
                    // Yeni akış açılamadıysa start_new_stream daha açıklayıcı bir hata yazmıştır
                    if s.last_error.is_none() {
                        s.last_error =
                            Some(format!("Ses cikisi kesildi ({reason}); ses aygiti degismis olabilir. Tekrar calmayi dene."));
                    }
                }
            }
        }
    }
}

async fn start_new_stream(
    client_opt: &mut Option<Client>,
    path: &Path,
    start_ms: u64,
    snapshot: &Arc<Mutex<PlayerSnapshot>>,
    pos_ms_atomic: &Arc<AtomicU64>,
    volume_atomic: &Arc<AtomicU32>,
    tap: &Arc<SampleTap>,
) -> Option<ActiveStream> {
    let mut opened = match open_audio_file(path) {
        Ok(o) => o,
        Err(e) => {
            if let Ok(mut s) = snapshot.lock() {
                s.status = PlaybackStatus::Idle;
                s.last_error = Some(format!("Dosya acilamadi: {e}"));
            }
            return None;
        }
    };
    // Kurtarmada şarkı kaldığı yerden devam eder
    let start_ms = if start_ms > 0 { seek_decoder(&mut opened.decoder_state, start_ms).unwrap_or(0) } else { 0 };

    if client_opt.is_none() {
        match Client::from_env(c"muzikit") {
            Ok(c) => *client_opt = Some(c),
            Err(e) => {
                if let Ok(mut s) = snapshot.lock() {
                    s.status = PlaybackStatus::Idle;
                    s.last_error = Some(format!("PipeWire/Pulse baglantisi kurulamadi: {e}"));
                }
                return None;
            }
        }
    }

    let channel_map = if opened.channels == 1 {
        protocol::ChannelMap::mono()
    } else {
        protocol::ChannelMap::stereo()
    };

    let params = protocol::PlaybackStreamParams {
        sample_spec: protocol::SampleSpec {
            format: protocol::SampleFormat::S16Le,
            channels: opened.channels,
            sample_rate: opened.sample_rate,
        },
        channel_map,
        cvolume: Some(protocol::ChannelVolume::norm(opened.channels)),
        sink_name: Some(protocol::DEFAULT_SINK.to_owned()),
        buffer_attr: protocol::stream::BufferAttr {
            target_length: (opened.sample_rate as f32 * opened.channels as f32 * 2.0 * TARGET_LATENCY_SECS) as u32,
            ..Default::default()
        },
        flags: protocol::stream::StreamFlags { adjust_latency: true, ..Default::default() },
        ..Default::default()
    };
    let channels = opened.channels as usize;
    let rate = opened.sample_rate;

    let duration_ms = opened.duration_ms;
    let decoder_arc = Arc::new(Mutex::new(opened.decoder_state));
    let stop_flag = Arc::new(AtomicBool::new(false));

    let dec_cb = Arc::clone(&decoder_arc);
    let stop_cb = Arc::clone(&stop_flag);
    let pos_cb = Arc::clone(pos_ms_atomic);
    let vol_cb = Arc::clone(volume_atomic);
    let tap_cb = Arc::clone(tap);
    tap.clear();

    let callback = move |buf: &mut [u8]| -> usize {
        if stop_cb.load(Ordering::Relaxed) {
            return 0;
        }

        let Ok(mut dec) = dec_cb.lock() else {
            return 0;
        };

        let vol = vol_cb.load(Ordering::Relaxed);
        let mut written = 0;

        while written < buf.len() {
            if dec.pcm_cursor >= dec.pcm_bytes.len()
                && (dec.eof || !fill_next_packet(&mut dec, vol)) {
                break;
            }

            let avail = dec.pcm_bytes.len() - dec.pcm_cursor;
            let need = buf.len() - written;
            let to_copy = avail.min(need);

            buf[written..written + to_copy]
                .copy_from_slice(&dec.pcm_bytes[dec.pcm_cursor..dec.pcm_cursor + to_copy]);

            dec.pcm_cursor += to_copy;
            written += to_copy;
        }

        pos_cb.store(dec.current_pos_ms, Ordering::Relaxed);
        tap_cb.push(&buf[..written], channels, rate, vol);
        written
    };

    let client = client_opt.as_ref()?;
    let stream = match client
        .create_playback_stream(params, callback.as_playback_source())
        .await
    {
        Ok(s) => s,
        Err(e) => {
            // Bağlantı koptuysa sıfırla
            *client_opt = None;
            if let Ok(mut s) = snapshot.lock() {
                s.status = PlaybackStatus::Idle;
                s.last_error = Some(format!("PipeWire ses akisi olusturulamadi: {e}"));
            }
            return None;
        }
    };

    let track_name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "Bilinmeyen".to_string());

    pos_ms_atomic.store(start_ms, Ordering::Relaxed);
    if let Ok(mut s) = snapshot.lock() {
        s.status = PlaybackStatus::Playing;
        s.track_name = Some(track_name);
        s.track_path = Some(path.to_path_buf());
        s.duration_ms = duration_ms;
        s.last_error = None;
    }

    let waiter = stream.clone();
    Some(ActiveStream {
        stream,
        done: Box::pin(async move { waiter.play_all().await.map_err(|e| e.to_string()) }),
        decoder: decoder_arc,
        stop_flag,
        paused: false,
        path: path.to_path_buf(),
        last_pos: start_ms,
        stalled_secs: 0,
        recovered_at: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_loop_mode_cycle() {
        assert_eq!(LoopMode::Off.next(), LoopMode::All);
        assert_eq!(LoopMode::All.next(), LoopMode::One);
        assert_eq!(LoopMode::One.next(), LoopMode::Off);
    }

    #[test]
    fn test_open_real_opus_if_present() {
        // Gerçek bir dosyayla denemek için: MUZIKIT_SAMPLE_OPUS=/yol/sarki.opus cargo test
        let Some(sample) = std::env::var_os("MUZIKIT_SAMPLE_OPUS").map(std::path::PathBuf::from) else { return };
        let sample = sample.as_path();
        if sample.exists() {
            let mut opened = open_audio_file(sample).expect("Opus dosyasi acilabilmeli");
            assert_eq!(opened.sample_rate, 48_000);
            assert!(opened.duration_ms > 10_000);
            assert!(fill_next_packet(&mut opened.decoder_state, 100));
            assert!(!opened.decoder_state.pcm_bytes.is_empty());
            let seeked_ms = seek_decoder(&mut opened.decoder_state, 15_000).expect("Seek calismali");
            assert!(seeked_ms >= 10_000 && seeked_ms <= 20_000);
        }
    }
}
