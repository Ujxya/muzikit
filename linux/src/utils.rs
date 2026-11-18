use regex::Regex;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::LazyLock;

/// Harici araçlar (yt-dlp, ffmpeg) için güvenli `Command` oluşturur:
/// - PATH'e sadece MUTLAK dizinler girer (boş/göreli girdiler = "mevcut dizin" saldırısını engeller),
/// - `$HOME` göreli/boşsa `~/.local/bin` eklenmez,
/// - stdin kapatılır (çocuk süreç TUI klavyesini okuyamaz).
pub fn safe_command(program: &str) -> Command {
    let mut cmd = Command::new(program);
    let mut dirs: Vec<PathBuf> = Vec::new();

    if let Some(home) = std::env::var_os("HOME") {
        let hp = Path::new(&home);
        if hp.is_absolute() {
            dirs.push(hp.join(".local/bin"));
            dirs.push(hp.join(".cargo/bin"));
        }
    }
    if let Some(p) = std::env::var_os("PATH") {
        for d in std::env::split_paths(&p) {
            if d.is_absolute() && !dirs.contains(&d) {
                dirs.push(d);
            }
        }
    }
    if dirs.is_empty() {
        dirs.extend(["/usr/local/bin", "/usr/bin", "/bin"].map(PathBuf::from));
    }
    if let Ok(joined) = std::env::join_paths(dirs) {
        cmd.env("PATH", joined);
    }
    cmd.stdin(Stdio::null());
    cmd
}

/// Kabul edilen en uzun URL (bellek/DoS koruması).
pub const MAX_URL_LEN: usize = 2048;

/// Sadece bu YouTube alan adlarına izin verilir (keyfi siteye / yerel dosyaya istek atılmasını engeller).
const ALLOWED_HOSTS: &[&str] = &[
    "youtube.com",
    "www.youtube.com",
    "m.youtube.com",
    "music.youtube.com",
    "youtu.be",
    "www.youtu.be",
];

static FILENAME_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    // İsim kısmı boş olabilir ("0373 - .opus"): yine de numaralı sayılır, yoksa "eksik numara" sanılır
    Regex::new(r"^(\d{4}) - .*\.opus$").unwrap()
});

/// Sadece ASCII karakterli 11 haneli video kimliği (Unicode `\w` kullanılmaz).
static VIDEO_ID_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?:[?&]v=|youtu\.be/|/embed/|/shorts/|/live/)([A-Za-z0-9_-]{11})(?:[^A-Za-z0-9_-]|$)",
    )
    .unwrap()
});

static PLAYLIST_ID_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"[?&]list=([A-Za-z0-9_-]{2,64})(?:[&#]|$)").unwrap()
});

static SANITIZE_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"[<>:"/\\|?*]"#).unwrap()
});

static BITRATE_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^([0-9]{1,3})k$").unwrap()
});

/// Bidi (yön değiştirme) karakterleri: dosya adı/terminal yanıltmasını (Trojan Source) engellemek için.
fn is_bidi_control(c: char) -> bool {
    matches!(c, '\u{200E}' | '\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}')
}

/// Terminal kaçış dizilerini (ANSI ESC), satır sonlarını ve diğer kontrol karakterlerini temizler.
/// Dış kaynaklı metinler (yt-dlp başlığı, stderr, dosya adları) ekrana basılmadan önce bundan geçer.
pub fn strip_control_chars(s: &str) -> String {
    s.chars()
        .map(|c| if c == '\t' || c == '\n' || c == '\r' { ' ' } else { c })
        .filter(|c| !c.is_control() && !is_bidi_control(*c))
        .collect()
}

/// Dosya adını işletim sisteminin yasakladığı karakterlerden, kontrol karakterlerinden
/// ve fazla boşluklardan temizler. Sonuç asla boş, `.` veya `..` olamaz.
pub fn sanitize_filename(name: &str, max_length: usize) -> String {
    let no_ctrl = strip_control_chars(name);
    let mut clean = SANITIZE_PATTERN.replace_all(&no_ctrl, "").to_string();

    // Fazla boşlukları tek boşluğa indir
    let words: Vec<&str> = clean.split_whitespace().collect();
    clean = words.join(" ");

    if clean.chars().count() > max_length {
        let truncated: String = clean.chars().take(max_length).collect();
        clean = match truncated.rmatch_indices(' ').next() {
            Some((idx, _)) => truncated[..idx].to_string(),
            None => truncated,
        };
    }

    let trimmed = clean.trim().trim_matches('.').trim().to_string();
    if trimmed.is_empty() {
        "Bilinmeyen".to_string()
    } else {
        trimmed
    }
}

/// Video kimliğinin tam olarak 11 ASCII karakter `[A-Za-z0-9_-]` olduğunu doğrular.
pub fn is_valid_video_id(id: &str) -> bool {
    id.len() == 11
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// URL'nin http(s) şemalı ve izin verilen bir YouTube alan adına ait olduğunu doğrular.
pub fn is_allowed_youtube_url(url: &str) -> bool {
    if url.is_empty()
        || url.len() > MAX_URL_LEN
        || url.chars().any(|c| c.is_control() || c.is_whitespace())
    {
        return false;
    }
    let lower = url.to_ascii_lowercase();
    let rest = if let Some(r) = lower.strip_prefix("https://") {
        r
    } else if let Some(r) = lower.strip_prefix("http://") {
        r
    } else {
        return false;
    };
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..end];
    // Kullanıcı bilgisi (user@host) veya port ile alan adı yanıltmasını reddet
    if authority.contains('@') || authority.contains(':') {
        return false;
    }
    ALLOWED_HOSTS.contains(&authority)
}

/// Şema yazılmamışsa (`youtube.com/...`) başına `https://` ekler.
fn with_scheme(url: &str) -> String {
    let t = url.trim();
    if t.contains("://") {
        t.to_string()
    } else {
        format!("https://{t}")
    }
}

/// YouTube linkinden 11 haneli video kimliğini çıkarır (sadece izin verilen alan adlarında).
pub fn extract_video_id(url: &str) -> Option<String> {
    let full = with_scheme(url);
    if !is_allowed_youtube_url(&full) {
        return None;
    }
    VIDEO_ID_PATTERN
        .captures(&full)
        .and_then(|caps| caps.get(1))
        .map(|m| m.as_str().to_string())
        .filter(|id| is_valid_video_id(id))
}

/// URL'yi doğrular ve normalize eder. Güvensiz / YouTube dışı URL için `None` döner.
/// Dönen URL her zaman sabit `https://www.youtube.com/...` biçimindedir; kullanıcı girdisi
/// yt-dlp'ye ham olarak hiç geçmez.
/// (normalize_url, is_playlist)
pub fn normalize_url(url: &str) -> Option<(String, bool)> {
    let full = with_scheme(url);
    if !is_allowed_youtube_url(&full) {
        return None;
    }

    if let Some(list_id) = PLAYLIST_ID_PATTERN
        .captures(&full)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
    {
        return Some((format!("https://www.youtube.com/playlist?list={list_id}"), true));
    }

    extract_video_id(&full).map(|vid| (format!("https://www.youtube.com/watch?v={vid}"), false))
}

/// FFmpeg bitrate değerini doğrular (örn: `128k`, 6k–510k arası).
pub fn is_valid_bitrate(bitrate: &str) -> bool {
    BITRATE_PATTERN
        .captures(bitrate)
        .and_then(|c| c.get(1))
        .and_then(|m| m.as_str().parse::<u32>().ok())
        .is_some_and(|v| (6..=510).contains(&v))
}

/// Bir dosya adının `0001 - ...opus` formatında olup olmadığını kontrol edip numarasını döner.
pub fn parse_track_number(filename: &str) -> Option<u32> {
    FILENAME_PATTERN.captures(filename).and_then(|caps| {
        caps.get(1).and_then(|m| m.as_str().parse::<u32>().ok())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_video_id() {
        assert_eq!(
            extract_video_id("https://www.youtube.com/watch?v=dQw4w9WgXcQ"),
            Some("dQw4w9WgXcQ".to_string())
        );
        assert_eq!(
            extract_video_id("https://youtu.be/dQw4w9WgXcQ?t=10"),
            Some("dQw4w9WgXcQ".to_string())
        );
        assert_eq!(
            extract_video_id("youtube.com/watch?v=dQw4w9WgXcQ&list=RDdQw4w9WgXcQ"),
            Some("dQw4w9WgXcQ".to_string())
        );
    }

    #[test]
    fn test_sanitize_filename() {
        let dirty = "Şarkı Adı: / Test ? * <Grup>";
        let clean = sanitize_filename(dirty, 100);
        assert_eq!(clean, "Şarkı Adı Test Grup");
    }

    #[test]
    fn test_sanitize_filename_blocks_traversal_and_escapes() {
        assert_eq!(sanitize_filename("../../etc/passwd", 100), "etcpasswd");
        assert_eq!(sanitize_filename("..", 100), "Bilinmeyen");
        assert_eq!(sanitize_filename("", 100), "Bilinmeyen");
        assert_eq!(sanitize_filename("a\x1b[31mred\x1b[0m\nb", 100), "a[31mred[0m b");
        assert_eq!(sanitize_filename("abc\u{202E}gpj.exe", 100), "abcgpj.exe");
    }

    #[test]
    fn test_parse_track_number() {
        assert_eq!(parse_track_number("0759 - test.opus"), Some(759));
        assert_eq!(parse_track_number("test.opus"), None);
        assert_eq!(parse_track_number("0373 - .opus"), Some(373));
        assert_eq!(parse_track_number("0373.opus"), None);
    }

    #[test]
    fn test_url_allowlist_and_injection() {
        // Seçenek enjeksiyonu (yt-dlp --exec) ve yabancı alan adları reddedilir
        assert_eq!(normalize_url("--exec=rm -rf ~"), None);
        assert_eq!(normalize_url("-o/tmp/x"), None);
        assert_eq!(normalize_url("https://evil.com/watch?v=dQw4w9WgXcQ"), None);
        assert_eq!(normalize_url("https://youtube.com.evil.com/watch?v=dQw4w9WgXcQ"), None);
        assert_eq!(normalize_url("https://youtube.com@evil.com/watch?v=dQw4w9WgXcQ"), None);
        assert_eq!(normalize_url("file:///etc/passwd"), None);
        assert_eq!(normalize_url("https://www.youtube.com/watch?v=dQw4w9WgXcQ\n--exec"), None);
        // Unicode harfli sahte ID kabul edilmez
        assert_eq!(extract_video_id("https://youtu.be/şşşşşşşşşşş"), None);
        // Geçerli girdiler sabit, güvenli biçime çevrilir
        assert_eq!(
            normalize_url("https://www.youtube.com/watch?v=us8FZU5H7so&list=RDus8FZU5H7so&t=986s"),
            Some(("https://www.youtube.com/playlist?list=RDus8FZU5H7so".to_string(), true))
        );
        assert_eq!(
            normalize_url("https://music.youtube.com/watch?v=dQw4w9WgXcQ"),
            Some(("https://www.youtube.com/watch?v=dQw4w9WgXcQ".to_string(), false))
        );
        // Geçersiz karakterli playlist ID'si playlist sayılmaz
        assert_eq!(
            normalize_url("https://www.youtube.com/watch?v=dQw4w9WgXcQ&list=ab;rm"),
            Some(("https://www.youtube.com/watch?v=dQw4w9WgXcQ".to_string(), false))
        );
    }

    #[test]
    fn test_video_id_and_bitrate_validation() {
        assert!(is_valid_video_id("dQw4w9WgXcQ"));
        assert!(!is_valid_video_id("dQw4w9WgXc"));
        assert!(!is_valid_video_id("dQw4w9Wg\nXc"));
        assert!(!is_valid_video_id("--exec=abcd"));
        assert!(is_valid_bitrate("128k"));
        assert!(!is_valid_bitrate("128k -y"));
        assert!(!is_valid_bitrate("9999k"));
        assert!(!is_valid_bitrate("-filter_complex"));
    }

    #[test]
    fn test_strip_control_chars() {
        assert_eq!(strip_control_chars("ok\x1b]0;pwn\x07\r\nx"), "ok]0;pwn  x");
    }
}
