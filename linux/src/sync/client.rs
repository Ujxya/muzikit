//! PC tarafı istemci: telefonu bulur, eşleşir, planı hazırlar ve uygular.
//! Bütün ağ işi ayrı iş parçacığında çalışır; ilerleme `log` ile bildirilir.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Write;
use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

use super::algo::{self, Item, Plan, Side};
use super::ledger::{self, Entry};
use super::proto;
use crate::utils::{is_valid_video_id, parse_track_number};

const JSON_LIMIT: u64 = 4 << 20;
const FILE_LIMIT: u64 = 100 << 20;
const KEY_REJECTED: &str = "eşleştirme anahtarı geçersiz";
const PC: Side = Side::L;
const PHONE: Side = Side::R;

#[derive(Clone, Debug)]
pub struct Peer {
    pub name: String,
    pub addr: SocketAddr,
}

/// mDNS ile `_muzikit._tcp` yayınlayan telefonları arar.
pub fn discover(wait: Duration) -> Result<Vec<Peer>> {
    let mdns = mdns_sd::ServiceDaemon::new().context("mDNS başlatılamadı")?;
    let rx = mdns.browse(proto::SERVICE).context("mDNS araması başlatılamadı")?;
    let deadline = Instant::now() + wait;
    let mut found: Vec<Peer> = Vec::new();
    while let Some(left) = deadline.checked_duration_since(Instant::now()) {
        let Ok(ev) = rx.recv_timeout(left) else { break };
        if let mdns_sd::ServiceEvent::ServiceResolved(info) = ev {
            let name = info.get_fullname().split('.').next().unwrap_or("telefon").to_string();
            for ip in info.get_addresses_v4() {
                let addr = SocketAddr::new(IpAddr::V4(ip), info.get_port());
                if proto::is_local(addr.ip()) && !found.iter().any(|p| p.addr == addr) {
                    found.push(Peer { name: name.clone(), addr });
                }
            }
        }
    }
    let _ = mdns.shutdown();
    Ok(found)
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .http_status_as_error(false)
        // Ortamdaki HTTP_PROXY yok sayılır: trafik yerel ağdan çıkmaz
        .proxy(None)
        .max_redirects(0)
        .timeout_global(Some(Duration::from_secs(300)))
        .build()
        .into()
}

#[derive(Debug)]
pub struct Hello {
    pub device: String,
    pub name: String,
    pub pairing: bool,
    pub salt: Option<Vec<u8>>,
}

pub fn hello(addr: SocketAddr, pc_id: &str) -> Result<Hello> {
    let mut r = agent()
        .get(&format!("http://{addr}/v1/hello?device={pc_id}"))
        .call()
        .context("telefona bağlanılamadı (aynı Wi-Fi'da ve eşitleme ekranı açık mı?)")?;
    if r.status().as_u16() != 200 {
        bail!("telefon {} döndü", r.status().as_u16());
    }
    let v: Value = serde_json::from_slice(&r.body_mut().with_config().limit(JSON_LIMIT).read_to_vec()?)?;
    if v["proto"].as_u64() != Some(2) {
        bail!("telefondaki muzikit sürümü uyumsuz (şifreli protokol 2 gerekli); iki tarafı da güncelle");
    }
    let device = v["device"].as_str().filter(|d| proto::is_device_id(d)).context("geçersiz telefon kimliği")?;
    let salt = match v["salt"].as_str() {
        Some(s) => Some(proto::unhex(s).filter(|b| b.len() == 16).context("geçersiz salt")?),
        None => None,
    };
    Ok(Hello {
        device: device.to_string(),
        name: crate::utils::strip_control_chars(v["name"].as_str().unwrap_or("telefon")).chars().take(64).collect(),
        pairing: v["pairing"].as_bool().unwrap_or(false),
        salt,
    })
}

pub struct Session {
    agent: ureq::Agent,
    addr: SocketAddr,
    key: [u8; 32],
    enc: [u8; 32],
    pc_id: String,
}

pub struct Reply {
    pub status: u16,
    pub body: Vec<u8>,
}

impl Session {
    /// İmzalı istek; yanıtın imzası doğrulanmazsa hata (yanıt atılır).
    /// İmzalı ve şifreli istek (protokol 2). GET gövdesizdir; POST/PUT gövdesi şifrelenir.
    /// Yanıtın imzası şifreli gövde üzerinden doğrulanır, sonra çözülür; olmazsa yanıt atılır.
    fn call(&self, method: &str, path: &str, body: Option<&[u8]>, limit: u64) -> Result<Reply> {
        let time = ledger::now_secs();
        let nonce = proto::random_hex(16)?;
        let payload = match method {
            "GET" => Vec::new(),
            _ => proto::encrypt(&self.enc, &proto::request_context(method, path, time, &nonce), body.unwrap_or(&[]))?,
        };
        let auth = proto::sign(&self.key, &proto::request_text(method, path, time, &nonce, &proto::sha_hex(&payload)));
        let url = format!("http://{}{path}", self.addr);
        let time_s = time.to_string();
        let headers = [
            ("X-Muzikit-Device", self.pc_id.as_str()),
            ("X-Muzikit-Time", time_s.as_str()),
            ("X-Muzikit-Nonce", nonce.as_str()),
            ("X-Muzikit-Auth", auth.as_str()),
            ("X-Muzikit-Enc", "aes256gcm-stream-v1"),
        ];
        let mut r = match method {
            "GET" => headers.iter().fold(self.agent.get(&url), |b, (k, v)| b.header(*k, *v)).call(),
            "POST" | "PUT" => {
                let b = if method == "POST" { self.agent.post(&url) } else { self.agent.put(&url) };
                headers
                    .iter()
                    .fold(b, |b, (k, v)| b.header(*k, *v))
                    .header("Content-Type", "application/octet-stream")
                    .send(&payload[..])
            }
            _ => bail!("desteklenmeyen yöntem"),
        }
        .with_context(|| format!("{method} {path} başarısız"))?;
        let status = r.status().as_u16();
        let get = |n: &str| r.headers().get(n).and_then(|v| v.to_str().ok()).map(str::to_string);
        let (rtime, rauth) = (get("X-Muzikit-Time").and_then(|t| t.parse::<i64>().ok()), get("X-Muzikit-Auth"));
        // Şifreleme her 64 KiB'a 16 bayt + 8 bayt önek ekler
        let wire_limit = limit + (limit / 65536 + 1) * 16 + 8;
        let raw = r.body_mut().with_config().limit(wire_limit).read_to_vec()?;
        let verified = rtime.is_some_and(|t| {
            proto::verify(&self.key, &proto::response_text(status, path, t, &nonce, &proto::sha_hex(&raw)), rauth.as_deref().unwrap_or(""))
        });
        if !verified {
            // İmzasız 401 = telefon bu anahtarı tanımıyor (eşleşme silinmiş)
            if status == 401 {
                bail!("telefon isteği reddetti (401): {KEY_REJECTED}");
            }
            bail!("yanıt imzası geçersiz: atıldı ({method} {path}, durum {status})");
        }
        let rtime = rtime.unwrap_or_default();
        if (ledger::now_secs() - rtime).abs() > 120 {
            bail!("yanıt zamanı tutarsız (iki cihazın saati doğru mu?)");
        }
        let body = proto::decrypt(&self.enc, &proto::response_context(status, path, rtime, &nonce), &raw)?;
        Ok(Reply { status, body })
    }

    fn json(&self, method: &str, path: &str, body: Option<&Value>) -> Result<Value> {
        let bytes = body.map(serde_json::to_vec).transpose()?;
        let r = self.call(method, path, bytes.as_deref(), JSON_LIMIT)?;
        let v: Value = serde_json::from_slice(&r.body).unwrap_or(Value::Null);
        if r.status != 200 {
            let err = v["error"].as_str().unwrap_or("?");
            bail!("{method} {path}: telefon {} döndü ({})", r.status, crate::utils::strip_control_chars(err));
        }
        Ok(v)
    }
}

/// Telefonun defter satırı (doğrulanmış).
#[derive(Clone, Debug)]
pub struct Remote {
    pub item: Item,
    pub video_id: Option<String>,
}

fn parse_index(v: &Value) -> Result<(Vec<Remote>, Vec<String>)> {
    let mut out = Vec::new();
    for it in v["items"].as_array().context("defter yok")? {
        let name = it["name"].as_str().unwrap_or("");
        let number = it["number"].as_u64().unwrap_or(u64::MAX);
        let sha = it["sha256"].as_str().unwrap_or("");
        let added_at = it["addedAt"].as_str().unwrap_or("");
        let added_by = it["addedBy"].as_str().unwrap_or("");
        if !ledger::is_song_name(name)
            || name.contains(['/', '\\'])
            || parse_track_number(name).map(u64::from) != Some(number)
            || !ledger::is_sha_hex(sha)
            || !ledger::is_valid_iso(added_at)
            || !matches!(added_by, "pc" | "phone")
        {
            bail!("telefon defterinde geçersiz satır: {}", crate::utils::strip_control_chars(name));
        }
        out.push(Remote {
            item: Item {
                number: number as u32,
                sha256: sha.into(),
                added_at: added_at.into(),
                added_by: added_by.into(),
                name: name.into(),
            },
            video_id: it["videoId"].as_str().filter(|i| is_valid_video_id(i)).map(str::to_string),
        });
    }
    let ids = v["downloaded"]
        .as_array()
        .context("downloaded yok")?
        .iter()
        .filter_map(Value::as_str)
        .filter(|i| is_valid_video_id(i))
        .map(str::to_string)
        .collect();
    Ok((out, ids))
}

fn index_json(device: &str, entries: &[Entry], downloaded: &[String]) -> Value {
    json!({
        "device": device,
        "items": entries.iter().map(|e| json!({
            "number": e.number, "sha256": e.sha256, "size": e.size, "videoId": e.video_id,
            "addedAt": e.added_at, "addedBy": e.added_by, "name": e.name,
        })).collect::<Vec<_>>(),
        "downloaded": downloaded,
    })
}

#[cfg(test)]
fn side_name(s: Side) -> &'static str {
    if s == PC { "pc" } else { "phone" }
}

fn parse_side(v: &Value) -> Result<Side> {
    match v.as_str() {
        Some("pc") => Ok(PC),
        Some("phone") => Ok(PHONE),
        _ => bail!("plan: geçersiz taraf"),
    }
}

fn num(v: &Value) -> Result<u32> {
    v.as_u64().and_then(|n| u32::try_from(n).ok()).context("plan: geçersiz numara")
}

fn s(v: &Value) -> Result<String> {
    v.as_str().map(str::to_string).context("plan: eksik alan")
}

/// Telefonun döndürdüğü planı bizim türlerimize çevirir.
fn parse_plan(v: &Value) -> Result<(Plan, Vec<String>)> {
    let mut p = Plan::default();
    for (key, to) in [("toPhone", PHONE), ("toPc", PC)] {
        for t in v[key].as_array().context("plan eksik")? {
            p.transfers.push(algo::Transfer { to, sha256: s(&t["sha256"])?, number: num(&t["number"])?, name: s(&t["name"])? });
        }
    }
    for r in v["renames"].as_array().context("plan eksik")? {
        p.renames.push(algo::Rename {
            side: parse_side(&r["side"])?,
            sha256: s(&r["sha256"])?,
            from: num(&r["from"])?,
            to: num(&r["to"])?,
            name: s(&r["name"])?,
        });
    }
    for m in v["mismatch"].as_array().context("plan eksik")? {
        p.mismatch.push(algo::Mismatch { sha256: s(&m["sha256"])?, l: num(&m["pc"])?, r: num(&m["phone"])? });
    }
    for d in v["duplicates"].as_array().context("plan eksik")? {
        p.duplicates.push(algo::Duplicate { side: parse_side(&d["side"])?, sha256: s(&d["sha256"])?, number: num(&d["number"])? });
    }
    p.normalize();
    let ids = v["downloaded"].as_array().context("plan eksik")?.iter().filter_map(Value::as_str).map(str::to_string).collect();
    Ok((p, ids))
}

/// downloaded.txt birleşimi: PC sırası esas, sonra PC'de olmayanlar telefonun sırasıyla.
pub fn merge_downloaded(pc: &[String], phone: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    pc.iter().chain(phone).filter(|i| seen.insert(i.as_str())).cloned().collect()
}

fn read_downloaded(path: &Path) -> Vec<String> {
    let mut seen = HashSet::new();
    fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|l| is_valid_video_id(l) && seen.insert(l.to_string()))
        .map(str::to_string)
        .collect()
}

/// Kullanıcıya gösterilecek, onay bekleyen hazır plan.
pub struct Prepared {
    pub phone_name: String,
    pub plan: Plan,
    pub downloaded: Vec<String>,
    /// downloaded.txt'ye eklenecek kimlik sayısı (bilgisayar, telefon)
    pub new_ids: (usize, usize),
    session: Session,
    music_dir: PathBuf,
    log_file: PathBuf,
    pc_entries: Vec<Entry>,
    phone: Vec<Remote>,
}

impl Prepared {
    pub fn count(&self, to: Side) -> usize {
        self.plan.transfers.iter().filter(|t| t.to == to).count()
    }
    pub fn to_phone(&self) -> usize {
        self.count(PHONE)
    }
    pub fn to_pc(&self) -> usize {
        self.count(PC)
    }
    pub fn needs_action(&self) -> bool {
        !self.plan.is_empty() || self.new_ids != (0, 0)
    }
}

pub enum Connect {
    Ready(Box<Prepared>),
    /// Saklı anahtar yok; telefon ekranındaki kod gerekiyor
    NeedCode { phone_name: String, pairing: bool },
}

/// Klasörü tarar, telefona bağlanır, iki planı hesaplayıp karşılaştırır. Dosyaya dokunmaz.
pub fn prepare(
    addr: SocketAddr,
    code: Option<&str>,
    music_dir: &Path,
    log_file: &Path,
    log: &dyn Fn(String),
) -> Result<Connect> {
    let pc_id = proto::device_id()?;
    let h = hello(addr, &pc_id)?;
    let (key, fresh) = match (proto::stored_key(&h.device), code) {
        (_, Some(code)) => {
            let salt = h.salt.as_ref().filter(|_| h.pairing).context("telefonda eşleştirme kodu açık değil")?;
            (proto::derive_key(code, salt, &h.device, &pc_id)?, true)
        }
        (Some(k), None) => (k, false),
        (None, None) => return Ok(Connect::NeedCode { phone_name: h.name, pairing: h.pairing }),
    };
    let session = Session { agent: agent(), addr, key, enc: proto::enc_key(&key), pc_id: pc_id.clone() };

    log(format!("Bağlandı: {}", h.name));
    let index = match session.json("GET", "/v1/index", None) {
        Ok(v) => v,
        Err(e) if !fresh && e.to_string().contains(KEY_REJECTED) => {
            // Telefon eşleşmeyi unutmuş olabilir: yeni kod istenir. Saklı anahtar burada SİLİNMEZ (ağdaki sahte
            // bir "telefon" imzasız 401 ile eşleşmeyi sildiremesin); kullanıcı kodu girince üzerine yazılır.
            return Ok(Connect::NeedCode { phone_name: h.name, pairing: h.pairing });
        }
        Err(e) if fresh => bail!("eşleştirme başarısız (kod yanlış olabilir): {e}"),
        Err(e) => return Err(e),
    };
    if fresh {
        proto::store_key(&h.device, &key)?;
        log("Eşleştirme tamamlandı, anahtar saklandı.".into());
    }
    let (phone, phone_ids) = parse_index(&index)?;

    log("Bilgisayar kütüphanesi taranıyor...".into());
    let mut last = Instant::now();
    let pc_entries = ledger::refresh(music_dir, |i, n| {
        if last.elapsed() > Duration::from_millis(500) {
            last = Instant::now();
            log(format!("  taranıyor {i}/{n}"));
        }
    })?;
    let pc_ids = read_downloaded(log_file);

    let pc_items: Vec<Item> = pc_entries
        .iter()
        .map(|e| Item {
            number: e.number,
            sha256: e.sha256.clone(),
            added_at: e.added_at.clone(),
            added_by: e.added_by.clone(),
            name: e.name.clone(),
        })
        .collect();
    let phone_items: Vec<Item> = phone.iter().map(|r| r.item.clone()).collect();
    let plan = algo::compute(&pc_items, &phone_items);
    let downloaded = merge_downloaded(&pc_ids, &phone_ids);

    log("Plan telefonla karşılaştırılıyor...".into());
    let theirs = session.json("POST", "/v1/plan", Some(&index_json(&pc_id, &pc_entries, &pc_ids)))?;
    let (their_plan, their_ids) = parse_plan(&theirs)?;
    if their_plan != plan || their_ids != downloaded {
        log(format!("PC planı: {plan:?}"));
        log(format!("Telefon planı: {their_plan:?}"));
        bail!("telefonun planı bizimkiyle aynı değil; hiçbir şey yapılmadı");
    }
    let new_ids = (downloaded.len() - pc_ids.len(), downloaded.len() - phone_ids.len());
    Ok(Connect::Ready(Box::new(Prepared {
        phone_name: h.name,
        plan,
        downloaded,
        new_ids,
        session,
        music_dir: music_dir.to_path_buf(),
        log_file: log_file.to_path_buf(),
        pc_entries,
        phone,
    })))
}

/// Üzerine yazmadan taşır (hedef varsa hata).
fn link_no_clobber(src: &Path, dest: &Path) -> Result<()> {
    match fs::hard_link(src, dest) {
        Ok(()) => {
            fs::remove_file(src)?;
            Ok(())
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => bail!("{} zaten var, üzerine yazılmadı", dest.display()),
        Err(_) => {
            if fs::symlink_metadata(dest).is_ok() {
                bail!("{} zaten var, üzerine yazılmadı", dest.display());
            }
            fs::rename(src, dest)?;
            Ok(())
        }
    }
}

/// Planın adını doğrular: sadece çıplak şarkı adı, klasör dışına çıkamaz.
fn safe_name(name: &str, number: u32) -> Result<&str> {
    if !ledger::is_song_name(name)
        || name.contains(['/', '\\'])
        || name.starts_with('.')
        || parse_track_number(name) != Some(number)
        || name.chars().any(|c| matches!(c, '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}'))
    {
        bail!("güvensiz dosya adı reddedildi");
    }
    Ok(name)
}

#[derive(Default, Debug)]
pub struct Outcome {
    pub received: usize,
    pub sent: usize,
    /// Bilgisayarda yeniden adlandırılanlar (eski ad, yeni ad): favoriler taşınır
    pub renamed: Vec<(String, String)>,
    pub downloaded_total: usize,
}

/// Onaylanan planı uygular: önce kaymalar, sonra aktarım, sonra downloaded.txt, en son kontrol + done.
pub fn execute(p: Prepared, log: &dyn Fn(String)) -> Result<Outcome> {
    let dir = &p.music_dir;
    let mut out = Outcome::default();
    let pc_by_sha: HashMap<&str, &Entry> = p.pc_entries.iter().map(|e| (e.sha256.as_str(), e)).collect();
    let phone_by_sha: HashMap<&str, &Remote> = p.phone.iter().map(|r| (r.item.sha256.as_str(), r)).collect();

    // 1) Kaymalar
    for r in &p.plan.renames {
        let new = safe_name(&r.name, r.to)?;
        if r.side == PC {
            let e = p
                .pc_entries
                .iter()
                .find(|e| e.sha256 == r.sha256 && e.number == r.from)
                .context("kayacak dosya bulunamadı")?;
            link_no_clobber(&dir.join(&e.name), &dir.join(new))?;
            ledger::upsert(dir, &dir.join(new), &e.sha256, e.video_id.clone(), &e.added_at, &e.added_by)?;
            out.renamed.push((e.name.clone(), new.to_string()));
            log(format!("Bilgisayarda kaydırıldı: {} → {new}", e.name));
        } else {
            p.session.json("POST", "/v1/rename", Some(&json!({ "sha256": r.sha256 })))?;
            log(format!("Telefonda kaydırıldı: {new}"));
        }
    }

    // 2) Aktarımlar
    for t in &p.plan.transfers {
        let name = safe_name(&t.name, t.number)?;
        if t.to == PC {
            let src = phone_by_sha.get(t.sha256.as_str()).context("telefon defterinde yok")?;
            log(format!("Alınıyor: {name}"));
            let r = p.session.call("GET", &format!("/v1/file/{}", t.sha256), None, FILE_LIMIT)?;
            if r.status != 200 {
                bail!("dosya alınamadı ({}): {name}", r.status);
            }
            if proto::sha_hex(&r.body) != t.sha256 {
                bail!("SHA tutmadı, dosya atıldı: {name}");
            }
            let part = dir.join(format!(".{:04}.part", t.number));
            // Yarıda kalmış önceki eşitlemenin geçici dosyası (bizim adlandırmamız) temizlenir
            if fs::symlink_metadata(&part).is_ok_and(|m| m.is_file() || m.file_type().is_symlink()) {
                fs::remove_file(&part)?;
            }
            {
                let mut f = fs::OpenOptions::new().write(true).create_new(true).open(&part)
                    .with_context(|| format!("{} oluşturulamadı", part.display()))?;
                f.write_all(&r.body)?;
                f.sync_all()?;
            }
            if let Err(e) = link_no_clobber(&part, &dir.join(name)) {
                let _ = fs::remove_file(&part);
                return Err(e);
            }
            ledger::upsert(dir, &dir.join(name), &t.sha256, src.video_id.clone(), &src.item.added_at, &src.item.added_by)?;
            out.received += 1;
        } else {
            let e = pc_by_sha.get(t.sha256.as_str()).context("bilgisayar defterinde yok")?;
            // Kaymış olabilir: güncel ad
            let cur = out.renamed.iter().find(|(o, _)| *o == e.name).map_or(e.name.as_str(), |(_, n)| n.as_str());
            let data = fs::read(dir.join(cur))?;
            if data.len() as u64 > FILE_LIMIT {
                bail!("dosya 100 MB sınırını aşıyor: {cur}");
            }
            if proto::sha_hex(&data) != t.sha256 {
                bail!("dosya eşitleme sırasında değişti: {cur}");
            }
            log(format!("Gönderiliyor: {name}"));
            let r = p.session.call("PUT", &format!("/v1/file/{}", t.sha256), Some(&data), JSON_LIMIT)?;
            if r.status != 200 {
                bail!("telefon dosyayı kabul etmedi ({}): {name}", r.status);
            }
            out.sent += 1;
        }
    }

    // 3) downloaded.txt (iki taraf aynı listeyi yazar)
    if read_downloaded(&p.log_file) != p.downloaded {
        let tmp = p.log_file.with_extension("txt.tmp");
        fs::write(&tmp, p.downloaded.iter().map(|i| format!("{i}\n")).collect::<String>())?;
        fs::rename(&tmp, &p.log_file)?;
    }
    p.session.json("POST", "/v1/downloaded", Some(&json!({ "ids": p.downloaded })))?;
    out.downloaded_total = p.downloaded.len();

    // 4) Son kontrol: SHA kümeleri aynı mı
    let pc_now: HashSet<String> = ledger::refresh(dir, |_, _| {})?.into_iter().map(|e| e.sha256).collect();
    let (phone_now, phone_ids) = parse_index(&p.session.json("GET", "/v1/index", None)?)?;
    let phone_now: HashSet<String> = phone_now.into_iter().map(|r| r.item.sha256).collect();
    let _ = p.session.json("POST", "/v1/done", Some(&json!({})));
    if pc_now != phone_now {
        bail!(
            "son kontrol: kütüphaneler farklı (yalnız PC: {}, yalnız telefon: {}); hiçbir şey silinmedi",
            pc_now.difference(&phone_now).count(),
            phone_now.difference(&pc_now).count()
        );
    }
    if phone_ids != p.downloaded {
        bail!("son kontrol: telefondaki downloaded.txt farklı");
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downloaded_merge_keeps_pc_order() {
        let pc = vec!["a".to_string(), "b".into()];
        let ph = vec!["c".to_string(), "a".into(), "d".into()];
        assert_eq!(merge_downloaded(&pc, &ph), ["a", "b", "c", "d"]);
    }

    #[test]
    fn rejects_unsafe_names() {
        assert!(safe_name("0817 - x.opus", 817).is_ok());
        assert!(safe_name("0817 - ../x.opus", 817).is_err());
        assert!(safe_name("0817 - x.opus", 818).is_err());
        assert!(safe_name("0817 - a\u{202E}sup.opus", 817).is_err());
        assert!(safe_name(".0817 - x.opus", 817).is_err());
    }

    #[test]
    fn plan_json_round_trip_matches() {
        let pc = vec![Item { number: 817, sha256: "a".repeat(64), added_at: "2026-11-17T10:00:00Z".into(), added_by: "phone".into(), name: "0817 - A.opus".into() }];
        let ph = vec![Item { number: 817, sha256: "b".repeat(64), added_at: "2026-11-17T11:00:00Z".into(), added_by: "pc".into(), name: "0817 - B.opus".into() }];
        let plan = algo::compute(&pc, &ph);
        // Telefonun döndüreceği JSON
        let v = json!({
            "toPhone": plan.transfers.iter().filter(|t| t.to == PHONE).map(|t| json!({"sha256": t.sha256, "number": t.number, "name": t.name})).collect::<Vec<_>>(),
            "toPc": plan.transfers.iter().filter(|t| t.to == PC).map(|t| json!({"sha256": t.sha256, "number": t.number, "name": t.name})).collect::<Vec<_>>(),
            "renames": plan.renames.iter().map(|r| json!({"side": side_name(r.side), "sha256": r.sha256, "from": r.from, "to": r.to, "name": r.name})).collect::<Vec<_>>(),
            "mismatch": [], "duplicates": [], "downloaded": ["x"],
        });
        let (p2, ids) = parse_plan(&v).unwrap();
        assert_eq!(p2, plan);
        assert_eq!(ids, ["x"]);
        assert_eq!(plan.renames[0].side, PHONE);
    }

    /// Uçtan uca: sahte telefon sunucusuna karşı (MUZIKIT_MOCK=port, MUZIKIT_PC_DIR=klasör).
    /// Normal test koşusunda çalışmaz.
    #[test]
    #[ignore]
    fn e2e_against_mock_phone() {
        let port: u16 = std::env::var("MUZIKIT_MOCK").unwrap().parse().unwrap();
        let dir = PathBuf::from(std::env::var("MUZIKIT_PC_DIR").unwrap());
        let addr: SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
        let log_file = dir.join("downloaded.txt");
        let log = |m: String| eprintln!("  {m}");
        let c = prepare(addr, None, &dir, &log_file, &log).unwrap();
        assert!(matches!(c, Connect::NeedCode { pairing: true, .. }));
        let Connect::Ready(p) = prepare(addr, Some("abcd-efgh-2345"), &dir, &log_file, &log).unwrap() else {
            panic!("hazır olmalıydı")
        };
        eprintln!("plan: {:#?}", p.plan);
        let o = execute(*p, &log).unwrap();
        eprintln!("sonuc: {o:?}");
        assert_eq!((o.received, o.sent, o.renamed.len()), (2, 1, 1));
    }

    /// Telefon eşitleme geçmişini silmiş (MOCK_FORGET=1): kod istenir; saklı anahtar sahte bir telefon
    /// tarafından sildirilemesin diye yerinde kalır (yeni kod girilince üzerine yazılır).
    #[test]
    #[ignore]
    fn e2e_phone_forgot_us() {
        let port: u16 = std::env::var("MUZIKIT_MOCK").unwrap().parse().unwrap();
        let dir = PathBuf::from(std::env::var("MUZIKIT_PC_DIR").unwrap());
        let addr: SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
        let log = |m: String| eprintln!("  {m}");
        assert!(proto::stored_key("0123456789abcdef").is_some(), "önce e2e_against_mock_phone koşmalı");
        let c = prepare(addr, None, &dir, &dir.join("downloaded.txt"), &log).unwrap();
        assert!(matches!(c, Connect::NeedCode { .. }));
        assert!(proto::stored_key("0123456789abcdef").is_some());
    }
}
