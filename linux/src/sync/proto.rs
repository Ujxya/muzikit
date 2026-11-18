//! Eşitleme protokolü: kimlik, anahtar, imza, yerel ağ denetimi.

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};

use super::ledger::hex;

pub const PORT: u16 = 47821;
pub const SERVICE: &str = "_muzikit._tcp.local.";
const CODE_ALPHABET: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

fn config_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(PathBuf::from).filter(|h| h.is_absolute()).map(|h| h.join(".config")))
        .map(|b| b.join("muzikit"))
}

/// Sadece sahibinin okuyabileceği (0600) dosya yazar, atomik.
fn write_private(path: &PathBuf, data: &[u8]) -> Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    if let Some(p) = path.parent() {
        fs::create_dir_all(p)?;
    }
    let tmp = path.with_extension("tmp");
    let _ = fs::remove_file(&tmp);
    let mut f = fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&tmp)?;
    f.write_all(data)?;
    f.sync_all()?;
    fs::rename(&tmp, path)?;
    Ok(())
}

pub fn is_device_id(s: &str) -> bool {
    s.len() == 16 && s.bytes().all(|c| matches!(c, b'0'..=b'9' | b'a'..=b'f'))
}

pub fn random_hex(n: usize) -> Result<String> {
    let mut b = vec![0u8; n];
    getrandom::fill(&mut b).map_err(|e| anyhow::anyhow!("rastgele sayı üretilemedi: {e}"))?;
    Ok(hex(&b))
}

/// Bu bilgisayarın kalıcı eşitleme kimliği (`~/.config/muzikit/sync-id`).
pub fn device_id() -> Result<String> {
    let path = config_dir().context("yapılandırma klasörü yok")?.join("sync-id");
    if let Ok(s) = fs::read_to_string(&path)
        && is_device_id(s.trim())
    {
        return Ok(s.trim().to_string());
    }
    let id = random_hex(8)?;
    write_private(&path, id.as_bytes())?;
    Ok(id)
}

fn keys_path() -> Result<PathBuf> {
    Ok(config_dir().context("yapılandırma klasörü yok")?.join("sync-key"))
}

fn load_keys() -> HashMap<String, String> {
    keys_path()
        .ok()
        .and_then(|p| fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// Telefon kimliği için saklı anahtar.
pub fn stored_key(phone_id: &str) -> Option<[u8; 32]> {
    let h = load_keys().remove(phone_id)?;
    unhex(&h)?.try_into().ok()
}

pub fn store_key(phone_id: &str, key: &[u8; 32]) -> Result<()> {
    let mut keys = load_keys();
    keys.insert(phone_id.to_string(), hex(key));
    write_private(&keys_path()?, serde_json::to_string(&keys)?.as_bytes())
}

pub fn unhex(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok()).collect()
}

/// `abcd-efgh 2345` → `ABCDEFGH2345`; geçersizse None.
pub fn normalize_code(code: &str) -> Option<String> {
    let c: String = code.chars().filter(|c| *c != '-' && !c.is_whitespace()).collect::<String>().to_ascii_uppercase();
    (c.len() == 12 && c.chars().all(|ch| CODE_ALPHABET.contains(ch))).then_some(c)
}

pub fn derive_key(code: &str, salt: &[u8], phone_id: &str, pc_id: &str) -> Result<[u8; 32]> {
    let code = normalize_code(code).context("kod 12 karakter olmalı (A–Z, 2–7)")?;
    let info = format!("muzikit-sync-v1|{phone_id}|{pc_id}");
    let mut okm = [0u8; 32];
    Hkdf::<Sha256>::new(Some(salt), code.as_bytes())
        .expand(info.as_bytes(), &mut okm)
        .map_err(|_| anyhow::anyhow!("HKDF"))?;
    Ok(okm)
}

/// Protokol 2 şifreleme anahtarı: HKDF(K, 32 sıfır bayt, "muzikit-sync-v1|enc").
pub fn enc_key(key: &[u8; 32]) -> [u8; 32] {
    let mut okm = [0u8; 32];
    Hkdf::<Sha256>::new(Some(&[0u8; 32]), key)
        .expand(b"muzikit-sync-v1|enc", &mut okm)
        .expect("32 bayt HKDF çıktısı her zaman geçerli");
    okm
}

const CHUNK: usize = 65536;
const TAG: usize = 16;

fn chunk_nonce(prefix: &[u8], seq: u32) -> [u8; 12] {
    let mut n = [0u8; 12];
    n[..8].copy_from_slice(prefix);
    n[8..].copy_from_slice(&seq.to_be_bytes());
    n
}

fn chunk_aad(context: &str, seq: u32, last: bool) -> Vec<u8> {
    format!("{context}|{seq}|{}", u8::from(last)).into_bytes()
}

/// Akış şifreleme: önek(8) ‖ AES-256-GCM parçaları (64 KiB düz metin).
pub fn encrypt_with_prefix(enc: &[u8; 32], context: &str, prefix: [u8; 8], plain: &[u8]) -> Vec<u8> {
    use aes_gcm::aead::{Aead, KeyInit, Payload};
    let cipher = aes_gcm::Aes256Gcm::new(enc.into());
    let chunks: Vec<&[u8]> = if plain.is_empty() { vec![&[][..]] } else { plain.chunks(CHUNK).collect() };
    let mut out = Vec::with_capacity(8 + plain.len() + TAG * chunks.len());
    out.extend_from_slice(&prefix);
    let last = chunks.len() - 1;
    for (i, c) in chunks.iter().enumerate() {
        let seq = i as u32;
        let aad = chunk_aad(context, seq, i == last);
        let ct = cipher
            .encrypt(&chunk_nonce(&prefix, seq).into(), Payload { msg: c, aad: &aad })
            .expect("AES-GCM şifreleme 64 KiB parçada başarısız olmaz");
        out.extend_from_slice(&ct);
    }
    out
}

pub fn encrypt(enc: &[u8; 32], context: &str, plain: &[u8]) -> Result<Vec<u8>> {
    let mut prefix = [0u8; 8];
    getrandom::fill(&mut prefix).map_err(|e| anyhow::anyhow!("rastgele sayı üretilemedi: {e}"))?;
    Ok(encrypt_with_prefix(enc, context, prefix, plain))
}

/// Akışı çözer; kesilmiş, sırası değişmiş ya da kurcalanmış gövde hata verir.
pub fn decrypt(enc: &[u8; 32], context: &str, body: &[u8]) -> Result<Vec<u8>> {
    use aes_gcm::aead::{Aead, KeyInit, Payload};
    if body.len() < 8 + TAG {
        bail!("şifreli gövde çok kısa");
    }
    let cipher = aes_gcm::Aes256Gcm::new(enc.into());
    let (prefix, mut rest) = body.split_at(8);
    let mut out = Vec::with_capacity(rest.len());
    let mut seq: u32 = 0;
    loop {
        let last = rest.len() <= CHUNK + TAG;
        let (c, tail) = rest.split_at(if last { rest.len() } else { CHUNK + TAG });
        if c.len() < TAG {
            bail!("şifreli gövde bozuk");
        }
        let aad = chunk_aad(context, seq, last);
        let pt = cipher
            .decrypt(&chunk_nonce(prefix, seq).into(), Payload { msg: c, aad: &aad })
            .map_err(|_| anyhow::anyhow!("gövde çözülemedi (anahtar yanlış ya da veri kurcalanmış)"))?;
        out.extend_from_slice(&pt);
        if last {
            return Ok(out);
        }
        rest = tail;
        seq = seq.checked_add(1).context("çok fazla parça")?;
    }
}

pub fn request_context(method: &str, path: &str, time: i64, nonce: &str) -> String {
    format!("req\n{method}\n{path}\n{time}\n{nonce}")
}

pub fn response_context(status: u16, path: &str, time: i64, req_nonce: &str) -> String {
    format!("resp\n{status}\n{path}\n{time}\n{req_nonce}")
}

pub fn sha_hex(data: &[u8]) -> String {
    hex(&Sha256::digest(data))
}

pub fn request_text(method: &str, path: &str, time: i64, nonce: &str, body_sha: &str) -> String {
    format!("{}\n{body_sha}", request_context(method, path, time, nonce))
}

pub fn response_text(status: u16, path: &str, time: i64, req_nonce: &str, body_sha: &str) -> String {
    format!("{}\n{body_sha}", response_context(status, path, time, req_nonce))
}

/// "Eşitleme geçmişini sil": eşleşme anahtarları ve cihaz kimliği silinir.
/// Müzik, downloaded.txt ve defter önbelleğine dokunmaz.
pub fn forget_all() -> Result<()> {
    let dir = config_dir().context("yapılandırma klasörü yok")?;
    for f in ["sync-key", "sync-id"] {
        match fs::remove_file(dir.join(f)) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.into()),
            _ => {}
        }
    }
    Ok(())
}

pub fn sign(key: &[u8; 32], text: &str) -> String {
    let mut m = <Hmac<Sha256> as Mac>::new_from_slice(key).expect("HMAC her uzunlukta anahtar alır");
    m.update(text.as_bytes());
    hex(&m.finalize().into_bytes())
}

/// Sabit zamanlı imza doğrulama.
pub fn verify(key: &[u8; 32], text: &str, auth_hex: &str) -> bool {
    let Some(tag) = unhex(auth_hex) else { return false };
    let mut m = <Hmac<Sha256> as Mac>::new_from_slice(key).expect("HMAC her uzunlukta anahtar alır");
    m.update(text.as_bytes());
    m.verify_slice(&tag).is_ok()
}

/// Sadece yerel ağ adresleri: 10/8, 172.16/12, 192.168/16, fe80::/10.
pub fn is_local(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v) => v.is_private(),
        IpAddr::V6(v) => (v.segments()[0] & 0xffc0) == 0xfe80,
    }
}

/// Kullanıcının yazdığı `192.168.1.5` ya da `192.168.1.5:47821`.
pub fn parse_manual(s: &str) -> Result<SocketAddr> {
    let s = s.trim();
    let addr: SocketAddr = match s.parse::<SocketAddr>() {
        Ok(a) => a,
        Err(_) => SocketAddr::new(s.parse::<IpAddr>().context("adres IP veya IP:port olmalı")?, PORT),
    };
    if !is_local(addr.ip()) {
        bail!("sadece yerel ağ adresleri kabul edilir");
    }
    Ok(addr)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_crypto_vectors() {
        let Some(doc) = crate::sync::test_vectors() else { return };
        let c = &doc["crypto"];
        let s = |k: &str| c[k].as_str().unwrap();
        let key = derive_key(s("code"), &unhex(s("salt")).unwrap(), s("phoneId"), s("pcId")).unwrap();
        assert_eq!(hex(&key), s("key"));

        let text = request_text("GET", "/v1/index", 1_800_000_000, "00112233445566778899aabbccddeeff", &sha_hex(b""));
        assert_eq!(text, c["request"]["text"].as_str().unwrap());
        assert_eq!(sign(&key, &text), c["request"]["auth"].as_str().unwrap());

        let body = c["response"]["body"].as_str().unwrap();
        let text = response_text(200, "/v1/index", 1_800_000_001, "00112233445566778899aabbccddeeff", &sha_hex(body.as_bytes()));
        assert_eq!(text, c["response"]["text"].as_str().unwrap());
        let auth = c["response"]["auth"].as_str().unwrap();
        assert!(verify(&key, &text, auth));
        assert!(!verify(&key, &text.replace("200", "201"), auth));
    }

    #[test]
    fn shared_encryption_vectors() {
        let Some(doc) = crate::sync::test_vectors() else { return };
        let e = &doc["encryption"];
        let s = |k: &str| e[k].as_str().unwrap();
        let k: [u8; 32] = unhex(doc["crypto"]["key"].as_str().unwrap()).unwrap().try_into().unwrap();
        let enc = enc_key(&k);
        assert_eq!(hex(&enc), s("encKey"));
        let ctx = s("context");
        let prefix: [u8; 8] = unhex(s("prefix")).unwrap().try_into().unwrap();

        let plain = e["small"]["plain"].as_str().unwrap().as_bytes();
        let ct = encrypt_with_prefix(&enc, ctx, prefix, plain);
        assert_eq!(hex(&ct), e["small"]["cipher"].as_str().unwrap());
        assert_eq!(decrypt(&enc, ctx, &ct).unwrap(), plain);

        let ct = encrypt_with_prefix(&enc, ctx, prefix, b"");
        assert_eq!(hex(&ct), e["empty"]["cipher"].as_str().unwrap());
        assert_eq!(ct.len(), 24);
        assert!(decrypt(&enc, ctx, &ct).unwrap().is_empty());

        let big: Vec<u8> = (0..65546u32).map(|i| (i * 7 % 251) as u8).collect();
        assert_eq!(sha_hex(&big), e["twoChunks"]["sha256Plain"].as_str().unwrap());
        let ct = encrypt_with_prefix(&enc, ctx, prefix, &big);
        assert_eq!(ct.len() as u64, e["twoChunks"]["cipherLength"].as_u64().unwrap());
        assert_eq!(sha_hex(&ct), e["twoChunks"]["sha256Cipher"].as_str().unwrap());
        assert_eq!(decrypt(&enc, ctx, &ct).unwrap(), big);

        // Kesme, kurcalama ve yanlış bağlam yakalanır
        assert!(decrypt(&enc, ctx, &ct[..ct.len() - 10]).is_err());
        assert!(decrypt(&enc, ctx, &ct[..8 + 65536 + 16]).is_err(), "son parça kesilirse ilk parça 'son' sanılmamalı");
        let mut bad = ct.clone();
        bad[100] ^= 1;
        assert!(decrypt(&enc, ctx, &bad).is_err());
        assert!(decrypt(&enc, &ctx.replace("POST", "PUT"), &ct).is_err());
        // Tam 64 KiB: tek parça
        let exact = vec![1u8; 65536];
        let ct = encrypt_with_prefix(&enc, ctx, prefix, &exact);
        assert_eq!(ct.len(), 8 + 65536 + 16);
        assert_eq!(decrypt(&enc, ctx, &ct).unwrap(), exact);
    }

    #[test]
    fn codes_and_addresses() {
        assert_eq!(normalize_code("abcd-efgh 2345").as_deref(), Some("ABCDEFGH2345"));
        assert!(normalize_code("ABCD-EFGH-2341").is_none()); // 1 base32'de yok
        assert!(normalize_code("ABCD").is_none());
        assert_eq!(parse_manual("192.168.1.5").unwrap().port(), PORT);
        assert_eq!(parse_manual("10.0.0.2:5000").unwrap().port(), 5000);
        assert!(parse_manual("8.8.8.8").is_err());
        assert!(parse_manual("127.0.0.1").is_err());
        assert!(parse_manual("evil.com").is_err());
    }
}
