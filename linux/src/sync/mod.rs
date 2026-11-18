//! Telefon ↔ bilgisayar eşitleme. Arayüz durumu burada; ağ işi ayrı iş parçacığında.

pub mod algo;
pub mod client;
pub mod ledger;
pub mod proto;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Duration;

use client::{Connect, Outcome, Peer, Prepared};

/// Android ile ortak test vektörleri (`linux/sync-test-vectors.json`). Dosya depoda yoksa bu testler atlanır.
#[cfg(test)]
pub fn test_vectors() -> Option<serde_json::Value> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("sync-test-vectors.json");
    match std::fs::read_to_string(&path) {
        Ok(s) => Some(serde_json::from_str(&s).expect("sync-test-vectors.json bozuk")),
        Err(_) => {
            eprintln!("sync-test-vectors.json yok, ortak vektör testi atlandı");
            None
        }
    }
}

pub enum Stage {
    Idle,
    Searching,
    Working,
    NeedCode { addr: SocketAddr, phone_name: String, pairing: bool },
    Ready(Box<Prepared>),
    Done(String),
    Failed(String),
}

enum Event {
    Log(String),
    Found(Vec<Peer>),
    Connected(SocketAddr, Connect),
    Finished(Outcome),
    Failed(String),
}

pub struct SyncState {
    pub stage: Stage,
    pub peers: Vec<Peer>,
    pub log: Vec<String>,
    pub manual: String,
    pub code: String,
    /// "Eşitleme geçmişini sil" onayı bekleniyor
    pub confirm_forget: bool,
    rx: Option<Receiver<Event>>,
}

impl Default for SyncState {
    fn default() -> Self {
        Self { stage: Stage::Idle, peers: Vec::new(), log: Vec::new(), manual: String::new(), code: String::new(), confirm_forget: false, rx: None }
    }
}

fn spawn(f: impl FnOnce(&Sender<Event>) + Send + 'static) -> Receiver<Event> {
    let (tx, rx) = channel();
    std::thread::spawn(move || f(&tx));
    rx
}

fn logger(tx: &Sender<Event>) -> impl Fn(String) + '_ {
    move |m| {
        let _ = tx.send(Event::Log(m));
    }
}

impl SyncState {
    pub fn busy(&self) -> bool {
        matches!(self.stage, Stage::Searching | Stage::Working)
    }

    pub fn search(&mut self) {
        if self.busy() {
            return;
        }
        self.stage = Stage::Searching;
        self.peers.clear();
        self.rx = Some(spawn(|tx| match client::discover(Duration::from_secs(4)) {
            Ok(p) => {
                let _ = tx.send(Event::Found(p));
            }
            Err(e) => {
                let _ = tx.send(Event::Failed(format!("{e:#}")));
            }
        }));
    }

    pub fn connect(&mut self, addr: SocketAddr, code: Option<String>, music_dir: PathBuf, log_file: PathBuf) {
        if self.busy() {
            return;
        }
        self.stage = Stage::Working;
        self.log.clear();
        self.rx = Some(spawn(move |tx| {
            match client::prepare(addr, code.as_deref(), &music_dir, &log_file, &logger(tx)) {
                Ok(c) => {
                    let _ = tx.send(Event::Connected(addr, c));
                }
                Err(e) => {
                    let _ = tx.send(Event::Failed(format!("{e:#}")));
                }
            }
        }));
    }

    pub fn approve(&mut self) {
        let Stage::Ready(p) = std::mem::replace(&mut self.stage, Stage::Working) else { return };
        self.rx = Some(spawn(move |tx| match client::execute(*p, &logger(tx)) {
            Ok(o) => {
                let _ = tx.send(Event::Finished(o));
            }
            Err(e) => {
                let _ = tx.send(Event::Failed(format!("{e:#}")));
            }
        }));
    }

    /// Eşleşme anahtarları ve cihaz kimliği silinir; sonraki eşitlemede yeniden kod gerekir.
    pub fn forget(&mut self) {
        if self.busy() {
            return;
        }
        self.confirm_forget = false;
        self.peers.clear();
        self.log.clear();
        self.code.clear();
        self.stage = match proto::forget_all() {
            Ok(()) => Stage::Done("esitleme gecmisi silindi: eslesmeler ve cihaz kimligi sifirlandi (muzikler duruyor)".into()),
            Err(e) => Stage::Failed(format!("silinemedi: {e:#}")),
        };
    }

    pub fn cancel(&mut self) {
        if !self.busy() {
            self.stage = Stage::Idle;
            self.code.clear();
        }
    }

    /// Kanaldaki olayları işler; eşitleme bittiyse sonucunu döner (kütüphane yenilensin diye).
    pub fn poll(&mut self) -> Option<Outcome> {
        let rx = self.rx.as_ref()?;
        let mut done = None;
        while let Ok(ev) = rx.try_recv() {
            match ev {
                Event::Log(m) => {
                    self.log.push(crate::utils::strip_control_chars(&m));
                    if self.log.len() > 200 {
                        self.log.remove(0);
                    }
                }
                Event::Found(p) => {
                    self.stage = Stage::Idle;
                    if p.is_empty() {
                        self.stage = Stage::Failed(
                            "telefon bulunamadi: telefonda esitleme ekrani acik mi? olmazsa IP adresini elle gir".into(),
                        );
                    }
                    self.peers = p;
                }
                Event::Connected(addr, Connect::NeedCode { phone_name, pairing }) => {
                    self.stage = Stage::NeedCode { addr, phone_name, pairing };
                }
                Event::Connected(_, Connect::Ready(p)) => {
                    self.code.clear();
                    self.stage = if p.needs_action() {
                        Stage::Ready(p)
                    } else {
                        Stage::Done(format!("{} ile zaten esit, yapilacak bir sey yok", p.phone_name))
                    };
                }
                Event::Finished(o) => {
                    self.stage = Stage::Done(format!(
                        "esitlendi: {} alindi, {} gonderildi, {} kaydirildi; downloaded.txt {} kayit",
                        o.received,
                        o.sent,
                        o.renamed.len(),
                        o.downloaded_total
                    ));
                    done = Some(o);
                }
                Event::Failed(e) => self.stage = Stage::Failed(e),
            }
        }
        if !self.busy() {
            self.rx = None;
        }
        done
    }
}
