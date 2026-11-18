//! Eksik karakterler için otomatik yazı tipi yedeği.
//!
//! Arayüz tek aralıklı bir Latin yazı tipiyle çizilir; Japonca, Korece, Arapça gibi
//! karakterler o yazı tipinde yoktur ve şarkı adları boş/kutu görünür. Ekrana gelecek
//! her metin buradan geçer: daha önce görülmemiş bir karakter mevcut yazı tiplerinde
//! yoksa fontconfig'e (`fc-match`, `fc-list`) o karakteri içeren yazı tipi sorulur,
//! dosya `ttf-parser` ile doğrulanır (karakter gerçekten var ve çizilebilir mi) ve
//! yazı tipi zincirlerinin sonuna eklenir. Her karakter yalnızca bir kez denetlenir.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use eframe::egui::{self, FontFamily, FontId};

use crate::utils::safe_command;

/// Tek seferde en fazla denenecek aday yazı tipi (fc-list sonucu çok uzun olabilir).
const MAX_CANDIDATES: usize = 24;

pub struct FontFallback {
    defs: egui::FontDefinitions,
    checked: HashSet<char>,
    loaded: HashSet<(PathBuf, u32)>,
    /// Hiçbir sistem yazı tipinde bulunamayanlar (tekrar aranmaz)
    pub unresolved: Vec<char>,
    added: usize,
}

/// Yazı tipi dosyası karakteri içeriyor ve egui'nin çizebileceği (vektör) biçimde mi?
/// Renkli bitmap emoji yazı tipleri (CBDT) egui'de çizilemediği için elenir.
fn face_covers(data: &[u8], index: u32, c: char) -> bool {
    match ttf_parser::Face::parse(data, index) {
        Ok(face) => {
            let t = face.tables();
            (t.glyf.is_some() || t.cff.is_some() || t.cff2.is_some()) && face.glyph_index(c).is_some()
        }
        Err(_) => false,
    }
}

fn parse_candidates(out: &[u8]) -> Vec<(PathBuf, u32)> {
    String::from_utf8_lossy(out)
        .lines()
        .filter_map(|l| {
            let (file, idx) = l.split_once('\t').unwrap_or((l, "0"));
            let file = file.trim();
            let p = PathBuf::from(file);
            // Sadece mutlak yollu, bilinen uzantılı yazı tipi dosyaları
            let ext = p.extension()?.to_string_lossy().to_ascii_lowercase();
            if !p.is_absolute() || !matches!(ext.as_str(), "ttf" | "otf" | "ttc" | "otc") {
                return None;
            }
            Some((p, idx.trim().parse().unwrap_or(0)))
        })
        .collect()
}

/// Karakteri içeren yazı tiplerini fontconfig'ten sırayla döndürür (en uygun önce).
fn candidates_for(c: char) -> Vec<(PathBuf, u32)> {
    let charset = format!(":charset={:x}", c as u32);
    let mut out = Vec::new();
    // 1) fontconfig'in en iyi eşleşmesi (sans-serif tercihli)
    if let Ok(o) = safe_command("fc-match")
        .args(["-f", "%{file}\t%{index}\n", &format!("sans-serif{charset}")])
        .output()
    {
        out.extend(parse_candidates(&o.stdout));
    }
    // 2) Karakteri içeren tüm yazı tipleri; düz (Regular) ve Sans olanlar öne
    if let Ok(o) = safe_command("fc-list").args(["--format", "%{file}\t%{index}\n", &charset]).output() {
        let mut all = parse_candidates(&o.stdout);
        all.sort_by_key(|(p, _)| {
            let n = p.to_string_lossy().to_ascii_lowercase();
            let mut score = 0;
            if !n.contains("regular") {
                score += 2;
            }
            if !n.contains("sans") {
                score += 1;
            }
            if ["bold", "black", "thin", "light", "italic", "condensed"].iter().any(|w| n.contains(w)) {
                score += 4;
            }
            score
        });
        out.extend(all);
    }
    let mut seen = HashSet::new();
    out.retain(|x| seen.insert(x.clone()));
    out.truncate(MAX_CANDIDATES);
    out
}

impl FontFallback {
    pub fn new(defs: egui::FontDefinitions) -> Self {
        Self { defs, checked: HashSet::new(), loaded: HashSet::new(), unresolved: Vec::new(), added: 0 }
    }

    /// Metindeki yeni karakterleri denetler; eksik olanlar için yazı tipi yükler.
    /// Yeni yazı tipi eklendiyse `true` (bir sonraki karede geçerli olur).
    pub fn ensure(&mut self, ctx: &egui::Context, text: &str) -> bool {
        let fresh: Vec<char> = text
            .chars()
            .filter(|c| !c.is_control() && !c.is_whitespace())
            .filter(|c| self.checked.insert(*c))
            .collect();
        if fresh.is_empty() {
            return false;
        }
        let fid = FontId::new(14.0, FontFamily::Monospace);
        let mut missing: Vec<char> = fresh.into_iter().filter(|c| !ctx.fonts(|f| f.has_glyph(&fid, *c))).collect();
        let mut changed = false;

        while let Some(&c) = missing.first() {
            let mut found = false;
            for (path, index) in candidates_for(c) {
                if self.loaded.contains(&(path.clone(), index)) {
                    continue;
                }
                let Ok(data) = std::fs::read(&path) else { continue };
                if !face_covers(&data, index, c) {
                    continue;
                }
                // Bu yazı tipinin karşıladığı diğer eksikler de çözülür
                missing.retain(|&m| !face_covers(&data, index, m));
                self.add_font(data, index);
                self.loaded.insert((path, index));
                changed = true;
                found = true;
                break;
            }
            if !found {
                self.unresolved.push(c);
                missing.retain(|&m| m != c);
            }
        }

        if changed {
            ctx.set_fonts(self.defs.clone());
        }
        changed
    }

    fn add_font(&mut self, data: Vec<u8>, index: u32) {
        let name = format!("fallback_{}", self.added);
        self.added += 1;
        let mut fd = egui::FontData::from_owned(data);
        fd.index = index;
        self.defs.font_data.insert(name.clone(), Arc::new(fd));
        for fam in [FontFamily::Monospace, FontFamily::Proportional, FontFamily::Name("bold".into())] {
            self.defs.families.entry(fam).or_default().push(name.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_fc_output_safely() {
        let out = b"/usr/share/fonts/a/NotoSansCJK-Regular.ttc\t2\nrelative.ttf\t0\n/etc/passwd\t0\n/x/y.TTF\n";
        let c = parse_candidates(out);
        assert_eq!(
            c,
            vec![
                (PathBuf::from("/usr/share/fonts/a/NotoSansCJK-Regular.ttc"), 2),
                (PathBuf::from("/x/y.TTF"), 0)
            ]
        );
    }

    #[test]
    fn rejects_garbage_font_data() {
        assert!(!face_covers(b"not a font", 0, 'a'));
    }
}
