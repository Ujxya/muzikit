//! Masaüstü arayüzü (egui / eframe).
//!
//! Eski TUI'nin görünümünü birebir korur: siyah zemin, hareketli siber arka plan,
//! beyaz yuvarlak dış çerçeve, gri kart menü, "pipewire calar" paneli —
//! ama fare, sürükle-bırak ilerleme çubuğu, kuyruk, favoriler gibi ek özelliklerle.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::time::{Duration, Instant};

use eframe::egui::{
    self, pos2, vec2, Align, Align2, Color32, CornerRadius, FontFamily, FontId, Key, Layout,
    Painter, Pos2, Rect, Response, RichText, Sense, Stroke, StrokeKind, TextStyle, Ui, UiBuilder,
};
use egui::text::{LayoutJob, TextWrapping};

use crate::app::{App, Screen, SleepTimer, SortMode, Theme};
use crate::fontfallback::FontFallback;
use crate::player::SampleTap;
use crate::spectrum::Spectrum;
use crate::keymap::{self, Action, Binding, Scope};
use crate::player::{LoopMode, PlaybackStatus};

// ── Renkler (TUI ile aynı siyah/beyaz/gri paleti) ─────────────────────
const BLACK: Color32 = Color32::from_rgb(0, 0, 0);
const WHITE: Color32 = Color32::from_rgb(238, 238, 238);
const GRAY: Color32 = Color32::from_rgb(168, 168, 168);
const DARK: Color32 = Color32::from_rgb(100, 100, 100);
const DIM: Color32 = Color32::from_rgb(48, 48, 48);
const HOVER: Color32 = Color32::from_rgba_premultiplied(20, 20, 20, 20);
const SEL: Color32 = Color32::from_rgba_premultiplied(36, 36, 36, 36);
// Görünüm ayarları her karede uygulama durumundan buraya kopyalanır; böylece serbest
// çizim fonksiyonlarına tek tek parametre taşımak gerekmez.
static FX_ON: AtomicBool = AtomicBool::new(true);
static PANEL_ALPHA: AtomicU8 = AtomicU8::new(125);

/// Menü/panel zemini: yarı saydam, arkadaki hareketli arka plan görünür.
fn panel_bg() -> Color32 {
    Color32::from_black_alpha(PANEL_ALPHA.load(Ordering::Relaxed))
}

const STAR_BRIGHT: &str = "✦";
const STAR_MID: &str = "✧";
const STAR_DIM: &str = "·";

const ROW_H: f32 = 23.0;
/// Ana menüdeki madde sayısı (indir, dönüştür, kütüphane, ayarlar, çıkış)
const MENU_ITEMS: usize = 5;
/// Alttaki "pipewire calar" panelinin yüksekliği (ana menü dahil her ekranda aynı)
const PLAYER_PANEL_H: f32 = 122.0;
const SEARCH_ID: &str = "muzikit_search";
const URL_ID: &str = "muzikit_url";

fn bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("bold".into()))
}
fn mono(size: f32) -> FontId {
    FontId::new(size, FontFamily::Monospace)
}

pub fn run_gui(app: App) -> anyhow::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("muzikit")
            .with_app_id("muzikit")
            .with_inner_size([1120.0, 740.0])
            .with_min_inner_size([780.0, 540.0]),
        // Kare hızını kendimiz belirliyoruz (request_repaint_after). vsync açıkken Wayland'de
        // pencere gizli/küçültülmüşken arabellek takası bloklanıp güncelleme döngüsü durabiliyor;
        // bu da şarkı bitince sonrakine geçmeyi ve medya tuşlarını geciktirir.
        vsync: false,
        ..Default::default()
    };
    eframe::run_native(
        "muzikit",
        options,
        Box::new(move |cc| {
            let font_defs = setup_style(&cc.egui_ctx);
            // MPRIS komutu gelince arayüz hemen uyansın (küçültülmüşken de)
            if let Some(m) = &app.mpris {
                let ctx = cc.egui_ctx.clone();
                m.set_waker(Box::new(move || ctx.request_repaint()));
            }
            Ok(Box::new(Gui::new(app, FontFallback::new(font_defs))))
        }),
    )
    .map_err(|e| anyhow::anyhow!("Arayuz baslatilamadi: {e}"))
}

// ── Yazı tipi ve tema ─────────────────────────────────────────────────
fn first_existing(paths: &[&str]) -> Option<Vec<u8>> {
    paths.iter().find_map(|p| std::fs::read(p).ok())
}

fn setup_style(ctx: &egui::Context) -> egui::FontDefinitions {
    let mut fonts = egui::FontDefinitions::default();

    // Sistemdeki tek aralıklı yazı tipleri (TUI hissi için). Bulunamazsa egui'nin Hack fontu kalır.
    let mono_font = first_existing(&[
        "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
        "/usr/share/fonts/TTF/DejaVuSansMono.ttf",
        "/usr/share/fonts/dejavu/DejaVuSansMono.ttf",
        "/usr/share/fonts/truetype/noto/NotoSansMono-Regular.ttf",
        "/usr/share/fonts/truetype/liberation/LiberationMono-Regular.ttf",
    ]);
    let bold_font = first_existing(&[
        "/usr/share/fonts/truetype/dejavu/DejaVuSansMono-Bold.ttf",
        "/usr/share/fonts/TTF/DejaVuSansMono-Bold.ttf",
        "/usr/share/fonts/dejavu/DejaVuSansMono-Bold.ttf",
        "/usr/share/fonts/truetype/noto/NotoSansMono-Bold.ttf",
        "/usr/share/fonts/truetype/liberation/LiberationMono-Bold.ttf",
    ]);
    // ✦ ✧ ♪ ▶ gibi simgeler için yedek
    let symbol_font = first_existing(&[
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/TTF/DejaVuSans.ttf",
        "/usr/share/fonts/truetype/noto/NotoSansSymbols2-Regular.ttf",
    ]);

    let mut mono_chain: Vec<String> = Vec::new();
    if let Some(bytes) = mono_font {
        fonts
            .font_data
            .insert("sys_mono".into(), Arc::new(egui::FontData::from_owned(bytes)));
        mono_chain.push("sys_mono".into());
    }
    mono_chain.extend(
        fonts
            .families
            .get(&FontFamily::Monospace)
            .cloned()
            .unwrap_or_default(),
    );
    if let Some(bytes) = symbol_font {
        fonts
            .font_data
            .insert("sys_sym".into(), Arc::new(egui::FontData::from_owned(bytes)));
        mono_chain.push("sys_sym".into());
    }

    let mut bold_chain = mono_chain.clone();
    if let Some(bytes) = bold_font {
        fonts
            .font_data
            .insert("sys_bold".into(), Arc::new(egui::FontData::from_owned(bytes)));
        bold_chain.insert(0, "sys_bold".into());
    }

    fonts.families.insert(FontFamily::Monospace, mono_chain.clone());
    fonts.families.insert(FontFamily::Proportional, mono_chain);
    fonts.families.insert(FontFamily::Name("bold".into()), bold_chain);
    ctx.set_fonts(fonts.clone());

    ctx.set_theme(egui::Theme::Dark);
    ctx.all_styles_mut(|style| {
        style.text_styles = [
            (TextStyle::Heading, bold(17.0)),
            (TextStyle::Body, mono(14.0)),
            (TextStyle::Monospace, mono(14.0)),
            (TextStyle::Button, mono(14.0)),
            (TextStyle::Small, mono(11.5)),
        ]
        .into();
        style.spacing.item_spacing = vec2(8.0, 6.0);
        style.spacing.button_padding = vec2(8.0, 3.0);
        style.spacing.interact_size.y = 22.0;

        let v = &mut style.visuals;
        v.dark_mode = true;
        v.panel_fill = BLACK;
        v.window_fill = BLACK;
        v.extreme_bg_color = Color32::from_black_alpha(200);
        v.faint_bg_color = Color32::from_rgb(12, 12, 12);
        v.code_bg_color = Color32::from_rgb(12, 12, 12);
        v.window_stroke = Stroke::new(1.0_f32, GRAY);
        v.window_corner_radius = CornerRadius::same(8);
        v.menu_corner_radius = CornerRadius::same(6);
        v.window_shadow = egui::epaint::Shadow::NONE;
        v.popup_shadow = egui::epaint::Shadow::NONE;
        v.hyperlink_color = WHITE;
        v.selection.bg_fill = Color32::from_rgb(70, 70, 70);
        v.selection.stroke = Stroke::new(1.0_f32, WHITE);
        v.text_cursor.stroke = Stroke::new(2.0_f32, WHITE);

        let w = &mut v.widgets;
        w.noninteractive.bg_fill = BLACK;
        w.noninteractive.weak_bg_fill = BLACK;
        w.noninteractive.bg_stroke = Stroke::new(1.0_f32, DIM);
        w.noninteractive.fg_stroke = Stroke::new(1.0_f32, GRAY);

        w.inactive.bg_fill = BLACK;
        w.inactive.weak_bg_fill = BLACK;
        w.inactive.bg_stroke = Stroke::new(1.0_f32, DARK);
        w.inactive.fg_stroke = Stroke::new(1.0_f32, GRAY);
        w.inactive.corner_radius = CornerRadius::same(5);

        w.hovered.bg_fill = HOVER;
        w.hovered.weak_bg_fill = HOVER;
        w.hovered.bg_stroke = Stroke::new(1.0_f32, GRAY);
        w.hovered.fg_stroke = Stroke::new(1.0_f32, WHITE);
        w.hovered.corner_radius = CornerRadius::same(5);
        w.hovered.expansion = 0.0;

        w.active.bg_fill = SEL;
        w.active.weak_bg_fill = SEL;
        w.active.bg_stroke = Stroke::new(1.0_f32, WHITE);
        w.active.fg_stroke = Stroke::new(1.0_f32, WHITE);
        w.active.corner_radius = CornerRadius::same(5);
        w.active.expansion = 0.0;

        w.open.bg_fill = SEL;
        w.open.weak_bg_fill = SEL;
        w.open.bg_stroke = Stroke::new(1.0_f32, GRAY);
        w.open.fg_stroke = Stroke::new(1.0_f32, WHITE);
        w.open.corner_radius = CornerRadius::same(5);
    });
    fonts
}

// ── Çizim yardımcıları ────────────────────────────────────────────────
fn format_ms(ms: u64) -> String {
    let s = ms / 1000;
    format!("{:02}:{:02}", s / 60, s % 60)
}

fn star_hash(x: u32, y: u32) -> u32 {
    // Basit tamsayı karması: her balığa sabit ama rastgele görünen özellikler verir
    let h = x.wrapping_mul(374_761_393)
        ^ y.wrapping_mul(668_265_263)
        ^ x.wrapping_add(y).wrapping_mul(1_442_695_041);
    (h ^ (h >> 13)).wrapping_mul(1_274_126_177)
}

// ── Siber arka plan ───────────────────────────────────────────────────
// Izgara, akan hex sütunları, kendini yazan sistem panelleri, radar, frekans analizörü
// ve sinyal dalgası; ana menüde üstüne çok soluk sabit CRT tarama çizgileri biner.
// Her şey zamana bağlı karmayla üretilir (durum yok). Titreme / bozulma efekti yoktur.

const NOISE: &[u8] = b"#%&@$01/\\|=+-:._<>";

fn hrand(a: u32, b: u32) -> f32 {
    (star_hash(a, b) % 10_000) as f32 / 10_000.0
}

fn gray(v: f32) -> Color32 {
    let v = v.clamp(0.0, 255.0) as u8;
    Color32::from_rgb(v, v, v)
}

fn noise_char(a: u32, b: u32) -> char {
    NOISE[(star_hash(a, b) as usize) % NOISE.len()] as char
}

fn panel_lines(i: u32, cyc: u32) -> Vec<String> {
    let h = |k: u32| star_hash(i * 977 + cyc, k);
    const HEADS: [&str; 6] = ["NET//MONITOR", "AUDIO//STREAM", "SYS//TRACE", "BUF//DECODE", "NODE//LINK", "SIG//SCAN"];
    let head = HEADS[(h(1) as usize) % HEADS.len()];
    let pct = h(2) % 101;
    let bars = (pct / 10) as usize;
    vec![
        format!("{head}  0x{:04X}", h(3) & 0xFFFF),
        format!("> node   {}.{}.{}.{}:{}", h(4) % 223 + 1, h(5) % 256, h(6) % 256, h(7) % 256, 1024 + h(8) % 60_000),
        format!("> trace  [{}{}] {pct:>3}%", "#".repeat(bars), "-".repeat(10 - bars)),
        format!("> pkt    {:>6}  loss {}.{}%", h(9) % 999_999, h(10) % 3, h(11) % 10),
        match h(12) % 5 {
            0 => "> opus   48000hz / stereo  [OK]".to_owned(),
            1 => format!("> key    {:08X}{:08X}", h(13), h(14)),
            2 => "> status SYNCING ...".to_owned(),
            3 => format!("> ERR    0x{:04X} :: retry", h(15) & 0xFFFF),
            _ => "> handshake ........ OK".to_owned(),
        },
    ]
}

fn corner_brackets(p: &Painter, r: Rect, len: f32, col: Color32) {
    let s = Stroke::new(1.0_f32, col);
    for (c, dx, dy) in [
        (r.left_top(), 1.0, 1.0),
        (r.right_top(), -1.0, 1.0),
        (r.left_bottom(), 1.0, -1.0),
        (r.right_bottom(), -1.0, -1.0),
    ] {
        p.line_segment([c, c + vec2(len * dx, 0.0)], s);
        p.line_segment([c, c + vec2(0.0, len * dy)], s);
    }
}

/// Köşe panellerinin konumları (panel yoksa boş).
fn panel_spots(area: Rect) -> Vec<Pos2> {
    let (pw, ph) = (250.0, 84.0);
    if area.width() <= 820.0 || area.height() <= 480.0 {
        return Vec::new();
    }
    vec![
        pos2(area.left() + 34.0, area.top() + 40.0),
        pos2(area.right() - pw - 34.0, area.top() + 40.0),
        // Alttakiler çalar panelinin üstünde kalsın
        pos2(area.left() + 34.0, area.bottom() - ph - PLAYER_PANEL_H - 70.0),
        pos2(area.right() - pw - 34.0, area.bottom() - ph - PLAYER_PANEL_H - 70.0),
    ]
}

/// `home`: ana menüde tüm öğeler; diğer ekranlarda yazıların altını kalabalıklaştırmamak
/// için sadece ızgara, veri sütunları ve dalga çizilir.
fn draw_background(p: &Painter, area: Rect, t: f32, home: bool, levels: &[f32]) {
    let spots = if home { panel_spots(area) } else { Vec::new() };
    // Hex sütunları panellerin üstüne binmesin
    let widgets = if home { widget_rects(area) } else { None };
    let mut keep_out: Vec<Rect> = spots
        .iter()
        .map(|&at| Rect::from_min_size(at - vec2(20.0, 18.0), vec2(250.0 + 40.0, 84.0 + 36.0)))
        .collect();
    // Ana menü kartının arkası: yarı saydam kartın altında yazı karmaşası olmasın
    if home {
        keep_out.push(Rect::from_center_size(area.center() - vec2(0.0, PLAYER_PANEL_H * 0.5), vec2(500.0, 360.0)));
    }
    // Alttaki çalar paneli (her ekranda): yazıları okunaklı kalsın
    keep_out.push(Rect::from_min_max(pos2(area.left(), area.bottom() - PLAYER_PANEL_H - 30.0), area.max));
    // Üstteki kayan veri şeridinin bandı
    keep_out.push(Rect::from_min_max(pos2(area.left(), area.top() + 12.0), pos2(area.right(), area.top() + 32.0)));
    if let Some((radar, spectrum)) = widgets {
        keep_out.push(radar.expand(16.0));
        keep_out.push(spectrum.expand(16.0));
    }

    // 1) İnce ızgara ve kesişimlerde artı işaretleri
    let step = 44.0;
    let grid = Color32::from_rgb(17, 18, 21);
    let x0 = area.left() + (area.width() % step) * 0.5;
    let y0 = area.top() + (area.height() % step) * 0.5;
    let (nx, ny) = ((area.width() / step) as u32 + 1, (area.height() / step) as u32 + 1);
    for i in 0..nx {
        let x = x0 + i as f32 * step;
        p.line_segment([pos2(x, area.top()), pos2(x, area.bottom())], Stroke::new(1.0_f32, grid));
    }
    for j in 0..ny {
        let y = y0 + j as f32 * step;
        p.line_segment([pos2(area.left(), y), pos2(area.right(), y)], Stroke::new(1.0_f32, grid));
    }
    let cross = Stroke::new(1.0_f32, Color32::from_rgb(62, 64, 70));
    for i in 0..nx {
        for j in 0..ny {
            if !star_hash(i, j).is_multiple_of(9) {
                continue;
            }
            let c = pos2(x0 + i as f32 * step, y0 + j as f32 * step);
            p.line_segment([c - vec2(3.0, 0.0), c + vec2(3.0, 0.0)], cross);
            p.line_segment([c - vec2(0.0, 3.0), c + vec2(0.0, 3.0)], cross);
        }
    }

    // Efektler kapalıysa sadece sabit ızgara kalır
    if !FX_ON.load(Ordering::Relaxed) {
        return;
    }

    // 2) Yukarı akan hex veri sütunları
    let font = mono(10.0);
    let lh = 13.0;
    let col_w = 120.0;
    let cols = (area.width() / col_w) as u32;
    for c in 0..cols {
        let h = star_hash(c, 21);
        if h.is_multiple_of(3) {
            continue;
        }
        let x = area.left() + 18.0 + c as f32 * col_w + (h % 40) as f32;
        let speed = 10.0 + ((h >> 8) % 25) as f32;
        let len = 6 + (h >> 4) % 14;
        let span = area.height() + len as f32 * lh;
        let head_y = area.bottom() - ((t * speed + ((h >> 12) % 1000) as f32) % span);
        for r in 0..len {
            let y = head_y + r as f32 * lh;
            if y < area.top() || y > area.bottom() {
                continue;
            }
            let cell = Rect::from_min_size(pos2(x, y), vec2(62.0, lh));
            if keep_out.iter().any(|r| r.intersects(cell)) {
                continue;
            }
            let tick = (t * 7.0) as u32 / (2 + r % 3);
            let v = star_hash(c * 131 + r, tick);
            let txt = format!("{:04X} {:02X}{:02X}", (v >> 16) & 0xFFFF, (v >> 8) & 0xFF, v & 0xFF);
            let fade = 1.0 - r as f32 / len as f32;
            let col = if r == 0 { gray(150.0) } else { gray(28.0 + 48.0 * fade) };
            p.text(pos2(x, y), Align2::LEFT_TOP, txt, font.clone(), col);
        }
    }

    // 3) Köşelerde kendini yazan sistem panelleri
    if home {
        let pf = mono(10.5);
        let (pw, ph) = (250.0, 84.0);
        for (i, &at) in spots.iter().enumerate() {
            let i = i as u32;
            let period = 6.0 + i as f32;
            let tt = t + i as f32 * 1.7;
            let cyc = (tt / period) as u32;
            let lt = tt - cyc as f32 * period;
            corner_brackets(p, Rect::from_min_size(at - vec2(8.0, 8.0), vec2(pw + 16.0, ph + 16.0)), 8.0, gray(85.0));

            let mut budget = (lt * 55.0) as usize; // daktilo efekti
            let lines = panel_lines(i, cyc);
            let mut cursor = None;
            for (k, line) in lines.iter().enumerate() {
                let y = at.y + k as f32 * 16.0;
                let n = line.chars().count();
                let shown: String = line.chars().take(budget).collect();
                let col = if k == 0 { gray(175.0) } else { gray(95.0) };
                p.text(pos2(at.x, y), Align2::LEFT_TOP, &shown, pf.clone(), col);
                if budget < n {
                    cursor = Some(pos2(at.x + shown.chars().count() as f32 * 6.4, y));
                    break;
                }
                budget -= n;
            }
            let cur = cursor.unwrap_or(pos2(at.x, at.y + lines.len() as f32 * 16.0));
            if ((t * 2.5) as u32).is_multiple_of(2) {
                p.text(cur, Align2::LEFT_TOP, "_", pf.clone(), gray(200.0));
            }
        }
    }

    // 4) Üstte kayan ikili veri şeridi
    if home {
        draw_ticker(p, area, t);
    }

    // 5) Radar ve frekans analizörü (geniş pencerelerde)
    if let Some((radar, spectrum)) = widgets {
        draw_radar(p, radar, t);
        draw_spectrum(p, spectrum, levels);
    }

    // 6) Ekranda dolaşıp hedefe kilitlenen nişangâh
    if home {
        draw_reticle(p, area, t, &keep_out);
    }

    // 7) Altta sinyal dalgası
    let base = area.bottom() - 34.0;
    let pts: Vec<Pos2> = (0..=(area.width() / 4.0) as u32)
        .map(|k| {
            let x = area.left() + k as f32 * 4.0;
            let u = x * 0.02;
            let env = 0.5 + 0.5 * (u * 0.23 - t * 0.8).sin();
            let mut y = (u + t * 3.0).sin() * 6.0 + (u * 2.7 - t * 5.0).sin() * 3.0;
            y *= 0.4 + env;
            pos2(x, base + y)
        })
        .collect();
    p.add(egui::Shape::line(pts, Stroke::new(1.0_f32, gray(70.0))));

    // 5) Rastgele parazit karakterleri
    let b = (t * 12.0) as u32;
    for k in 0..40 {
        let q = pos2(
            area.left() + hrand(b * 61 + k, 60) * area.width(),
            area.top() + hrand(b * 61 + k, 61) * area.height(),
        );
        let ch = noise_char(b * 61 + k, 62).to_string();
        p.text(q, Align2::CENTER_CENTER, ch, font.clone(), gray(35.0 + 40.0 * hrand(k, b)));
    }
}

/// Ana menüde içeriğin üstüne binen katman: sadece sabit, çok soluk CRT tarama çizgileri.
fn draw_scanlines(p: &Painter, area: Rect) {
    let sc = Stroke::new(1.0_f32, Color32::from_black_alpha(48));
    let mut y = area.top();
    while y < area.bottom() {
        p.line_segment([pos2(area.left(), y), pos2(area.right(), y)], sc);
        y += 3.0;
    }
}

/// Radar (sol orta) ve frekans analizörü (sağ orta) için yer; dar pencerede yok.
fn widget_rects(area: Rect) -> Option<(Rect, Rect)> {
    if area.width() < 1000.0 || area.height() < 600.0 {
        return None;
    }
    // Çalar panelinin üstünde kalan alanın ortası
    let cy = (area.top() + area.bottom() - PLAYER_PANEL_H) * 0.5;
    let radar = Rect::from_center_size(pos2(area.left() + 150.0, cy), vec2(150.0, 150.0));
    let spectrum = Rect::from_min_size(pos2(area.right() - 250.0, cy - 50.0), vec2(200.0, 100.0));
    Some((radar, spectrum))
}

/// Dönen tarama çizgisi ve geçtikçe parlayan hedef noktaları olan radar.
fn draw_radar(p: &Painter, rect: Rect, t: f32) {
    use std::f32::consts::TAU;
    let c = rect.center();
    let r = rect.width() * 0.5;
    let ring = Stroke::new(1.0_f32, gray(48.0));
    for k in 1..=3 {
        p.circle_stroke(c, r * k as f32 / 3.0, ring);
    }
    p.line_segment([c - vec2(r, 0.0), c + vec2(r, 0.0)], ring);
    p.line_segment([c - vec2(0.0, r), c + vec2(0.0, r)], ring);

    let sweep = (t * 1.6) % TAU;
    for k in 0..14 {
        let a = sweep - k as f32 * 0.05;
        let col = gray(170.0 * (1.0 - k as f32 / 14.0));
        p.line_segment([c, c + vec2(a.cos(), a.sin()) * r], Stroke::new(1.0_f32, col));
    }

    let font = mono(9.5);
    let cyc = (t / 8.0) as u32;
    for i in 0..6 {
        let ang = hrand(cyc * 17 + i, 70) * TAU;
        let dist = (0.2 + 0.75 * hrand(cyc * 17 + i, 71)) * r;
        let since = (sweep - ang).rem_euclid(TAU);
        let glow = (1.0 - since / TAU).powi(3);
        let q = c + vec2(ang.cos(), ang.sin()) * dist;
        p.circle_filled(q, 1.5 + glow * 1.5, gray(40.0 + 215.0 * glow));
        if glow > 0.55 {
            p.text(q + vec2(5.0, -5.0), Align2::LEFT_BOTTOM, format!("{:02X}", star_hash(cyc, i) & 0xFF), font.clone(), gray(200.0 * glow));
        }
    }
    p.text(rect.left_top() - vec2(0.0, 10.0), Align2::LEFT_BOTTOM, "RADAR//PASSIVE", mono(10.5), gray(175.0));
    p.text(
        rect.left_bottom() + vec2(0.0, 10.0),
        Align2::LEFT_TOP,
        format!("AZ {:03.0}°  RNG {:.1}km", sweep.to_degrees(), 2.0 + 3.0 * hrand(cyc, 72)),
        mono(10.0),
        gray(95.0),
    );
}

/// LED tarzı bölmeli çubuklarla frekans analizörü; `levels` çalan müzikten gelir (0..1).
fn draw_spectrum(p: &Painter, rect: Rect, levels: &[f32]) {
    let n = levels.len().max(1);
    let gap = 2.0;
    let bw = (rect.width() - gap * (n - 1) as f32) / n as f32;
    let seg = 4.0;
    let max_segs = (rect.height() / seg) as u32;
    for (i, &level) in levels.iter().enumerate() {
        // Sessizlikte de bir bölme görünsün (analizör kaybolmasın)
        let segs = ((max_segs as f32 * level).round() as u32).clamp(1, max_segs);
        let x = rect.left() + i as f32 * (bw + gap);
        for k in 0..segs {
            let y = rect.bottom() - (k + 1) as f32 * seg;
            let top = k + 1 == segs && segs > 1;
            let col = if top { gray(210.0) } else { gray(60.0 + 90.0 * k as f32 / max_segs as f32) };
            p.rect_filled(Rect::from_min_size(pos2(x, y), vec2(bw, seg - 1.0)), CornerRadius::ZERO, col);
        }
    }
    p.text(rect.left_top() - vec2(0.0, 10.0), Align2::LEFT_BOTTOM, "FREQ//ANALYZER", mono(10.5), gray(175.0));
    p.text(rect.left_bottom() + vec2(0.0, 8.0), Align2::LEFT_TOP, "30Hz", mono(9.5), gray(85.0));
    p.text(rect.right_bottom() + vec2(0.0, 8.0), Align2::RIGHT_TOP, "16kHz", mono(9.5), gray(85.0));
}

/// Üst kenarda sola doğru kayan ikili/hex veri şeridi.
fn draw_ticker(p: &Painter, area: Rect, t: f32) {
    let font = mono(10.0);
    let cw = 6.1;
    let y = area.top() + 22.0;
    let x0 = area.left() + 300.0;
    let x1 = area.right() - 30.0;
    if x1 - x0 < 100.0 {
        return;
    }
    let shift = t * 40.0;
    let first = (shift / cw) as u32;
    let frac = shift % cw;
    let n = ((x1 - x0) / cw) as u32;
    let text: String = (first..first + n)
        .map(|i| match i % 26 {
            8 | 17 => ' ',
            24 | 25 => ':',
            _ => if star_hash(i, 81) & 1 == 0 { '0' } else { '1' },
        })
        .collect();
    p.text(pos2(x0 - frac, y), Align2::LEFT_CENTER, text, font, gray(58.0));
}

/// Ekranda hedeften hedefe atlayan, yaklaşırken daralan kilitlenme nişangâhı.
/// `avoid`: nişangâh ve etiketinin üstüne binmemesi gereken alanlar (kart, radar, paneller...).
fn draw_reticle(p: &Painter, area: Rect, t: f32, avoid: &[Rect]) {
    let period = 3.6;
    let cyc = (t / period) as u32;
    let lt = t - cyc as f32 * period;
    // Boş bir yer bulana kadar birkaç aday dene; bulunamazsa bu turda gösterme
    let Some(c) = (0..12u32).find_map(|k| {
        let c = pos2(
            area.left() + 60.0 + hrand(cyc * 13 + k, 90) * (area.width() - 260.0),
            area.top() + 60.0 + hrand(cyc * 13 + k, 91) * (area.height() - 120.0),
        );
        // Nişangâh + sağ üstteki etiket
        let footprint = Rect::from_min_max(c - vec2(40.0, 60.0), c + vec2(190.0, 40.0));
        (!avoid.iter().any(|r| r.intersects(footprint))).then_some(c)
    }) else {
        return;
    };
    let lock = (lt / 0.6).min(1.0);
    let size = 70.0 - 44.0 * lock;
    let col = if lock < 1.0 { gray(110.0) } else { gray(200.0) };
    corner_brackets(p, Rect::from_center_size(c, vec2(size, size)), 7.0, col);
    let s = Stroke::new(1.0_f32, col);
    p.line_segment([c - vec2(5.0, 0.0), c + vec2(5.0, 0.0)], s);
    p.line_segment([c - vec2(0.0, 5.0), c + vec2(0.0, 5.0)], s);
    if lock >= 1.0 {
        let tag = c + vec2(size * 0.5 + 18.0, -size * 0.5 - 14.0);
        p.line_segment([c + vec2(size * 0.5, -size * 0.5), tag], Stroke::new(1.0_f32, gray(90.0)));
        let font = mono(9.5);
        p.text(tag + vec2(3.0, 0.0), Align2::LEFT_BOTTOM, format!("LOCK  ID {:04X}-{:02X}", star_hash(cyc, 92) & 0xFFFF, star_hash(cyc, 93) & 0xFF), font.clone(), gray(190.0));
        p.text(tag + vec2(3.0, 1.0), Align2::LEFT_TOP, format!("dist {:.1}m  conf {}%", 5.0 + 90.0 * hrand(cyc, 94), 70 + star_hash(cyc, 95) % 30), font, gray(100.0));
    }
}

fn titled_box(p: &Painter, rect: Rect, title: &str, stroke: Color32, title_color: Color32) {
    p.rect_filled(rect, CornerRadius::same(8), panel_bg());
    p.rect_stroke(rect, CornerRadius::same(8), Stroke::new(1.0_f32, stroke), StrokeKind::Inside);
    if !title.is_empty() {
        let g = p.layout_no_wrap(title.to_string(), bold(13.0), title_color);
        let pos = pos2(rect.left() + 14.0, rect.top() - g.size().y * 0.5);
        p.rect_filled(
            Rect::from_min_size(pos - vec2(5.0, 0.0), g.size() + vec2(10.0, 0.0)),
            CornerRadius::ZERO,
            BLACK,
        );
        p.galley(pos, g, title_color);
    }
}

fn text_trunc(p: &Painter, left_center: Pos2, text: &str, font: FontId, color: Color32, max_w: f32) -> f32 {
    if max_w <= 4.0 {
        return 0.0;
    }
    let mut job = LayoutJob::simple_singleline(text.to_string(), font, color);
    job.wrap = TextWrapping::truncate_at_width(max_w);
    let g = p.layout_job(job);
    let w = g.size().x;
    p.galley(pos2(left_center.x, left_center.y - g.size().y * 0.5), g, color);
    w
}

/// Konsol tarzı düğme: siyah zemin, ince gri kenar.
fn tbtn(ui: &mut Ui, label: &str) -> Response {
    ui.add(egui::Button::new(RichText::new(label).color(GRAY)))
}

/// İnce çizgi tarzı kaydırıcı (temaya uygun); değer değiştiyse `true`.
fn line_slider(ui: &mut Ui, value: &mut f32, min: f32, max: f32, step: f32) -> bool {
    let (rect, resp) = ui.allocate_exact_size(vec2(220.0, 18.0), Sense::click_and_drag());
    let old = *value;
    if let Some(pos) = resp.interact_pointer_pos() {
        let f = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
        *value = ((min + f * (max - min)) / step).round() * step;
    }
    let f = ((*value - min) / (max - min)).clamp(0.0, 1.0);
    let p = ui.painter();
    let y = rect.center().y;
    let x = rect.left() + f * rect.width();
    p.line_segment([pos2(rect.left(), y), pos2(rect.right(), y)], Stroke::new(1.0_f32, DARK));
    p.line_segment([pos2(rect.left(), y), pos2(x, y)], Stroke::new(2.0_f32, WHITE));
    for k in 0..=10 {
        let tx = rect.left() + k as f32 / 10.0 * rect.width();
        p.line_segment([pos2(tx, y - 3.0), pos2(tx, y + 3.0)], Stroke::new(1.0_f32, DIM.gamma_multiply(2.0)));
    }
    let knob = if resp.hovered() || resp.dragged() { WHITE } else { GRAY };
    p.rect_filled(Rect::from_center_size(pos2(x, y), vec2(4.0, 12.0)), CornerRadius::ZERO, knob);
    (*value - old).abs() > f32::EPSILON
}

/// Minimal aç/kapa düğmesi: "[x] etiket" / "[ ] etiket".
fn toggle(ui: &mut Ui, on: bool, label: &str) -> Response {
    let mark = if on { "[x]" } else { "[ ]" };
    let text = if label.is_empty() { mark.to_owned() } else { format!("{mark} {label}") };
    ui.add(egui::Button::new(RichText::new(text).font(mono(13.0)).color(if on { WHITE } else { DARK })).frame(false))
}

fn tbtn_on(ui: &mut Ui, label: &str, on: bool) -> Response {
    let text = if on {
        RichText::new(label).color(WHITE)
    } else {
        RichText::new(label).color(DARK)
    };
    let mut b = egui::Button::new(text);
    if on {
        b = b.stroke(Stroke::new(1.0_f32, GRAY));
    }
    ui.add(b)
}

struct BarOut {
    /// Tıklama/sürükleme bittiğinde seçilen oran
    committed: Option<f32>,
    /// Sürükleme sürerken anlık oran
    live: Option<f32>,
}

/// TUI'deki `━━━━●─────` ilerleme çubuğunun tıklanabilir/sürüklenebilir hali.
fn progress_bar(ui: &mut Ui, width: f32, frac: f32, drag: &mut Option<f32>, enabled: bool, tooltip: impl Fn(f32) -> String) -> BarOut {
    let sense = if enabled { Sense::click_and_drag() } else { Sense::hover() };
    let (rect, resp) = ui.allocate_exact_size(vec2(width.max(30.0), 18.0), sense);
    let frac_at = |p: Pos2| ((p.x - rect.left()) / rect.width()).clamp(0.0, 1.0);

    let mut out = BarOut { committed: None, live: None };
    if enabled {
        if resp.dragged()
            && let Some(p) = resp.interact_pointer_pos() {
            *drag = Some(frac_at(p));
            out.live = *drag;
        }
        if resp.drag_stopped() {
            out.committed = drag.take();
        } else if resp.clicked() {
            out.committed = resp.interact_pointer_pos().map(frac_at);
            *drag = None;
        }
    }
    let shown = drag.unwrap_or(out.committed.unwrap_or(frac)).clamp(0.0, 1.0);

    let p = ui.painter();
    let y = rect.center().y;
    let x = rect.left() + rect.width() * shown;
    p.line_segment([pos2(rect.left(), y), pos2(rect.right(), y)], Stroke::new(1.0_f32, DARK));
    if enabled {
        p.line_segment([pos2(rect.left(), y), pos2(x, y)], Stroke::new(3.0_f32, WHITE));
        let r = if resp.hovered() || drag.is_some() { 6.0 } else { 4.5 };
        p.circle_filled(pos2(x, y), r, WHITE);
    }
    if enabled
        && let Some(hp) = resp.hover_pos() {
        let f = frac_at(hp);
        p.line_segment([pos2(hp.x, y - 6.0), pos2(hp.x, y + 6.0)], Stroke::new(1.0_f32, GRAY));
        resp.on_hover_text_at_pointer(tooltip(f));
    }
    out
}

enum KeyEv {
    Key(Key, egui::Modifiers),
    Text(String),
}

const SPINNER: [&str; 4] = [STAR_DIM, STAR_MID, STAR_BRIGHT, STAR_MID];

enum RowAction {
    Select(usize),
    Play(String),
    Enqueue(String, bool),
    Fav(String),
    Copy(String),
    OpenDir,
}

// ── Uygulama ──────────────────────────────────────────────────────────
struct Gui {
    app: App,
    start: Instant,
    seek_drag: Option<f32>,
    vol_drag: Option<f32>,
    focus_search: bool,
    focus_url: bool,
    list_offset: f32,
    list_view_h: f32,
    last_title: String,
    /// Yeni tuş beklenen kısayol: (eylem, yuva)
    capture: Option<(Action, usize)>,
    /// Ayarlar & Kısayollar ekranındaki sekme: 0 ayarlar (en altta kütüphane raporu), 1 kısayollar
    report_tab: u8,
    help_open: bool,
    last_screen: Screen,
    screen_changed: Instant,
    /// Eksik karakterler (Japonca vb.) için otomatik yazı tipi yedeği
    fonts: FontFallback,
    seen_library_gen: u64,
    seen_log_gen: u64,
    /// Gerçek frekans analizörü: çalan sesin örnekleri ve çubuk durumu
    tap: Arc<SampleTap>,
    spectrum: Spectrum,
    last_spectrum: Instant,
    /// "Balık göleti" temasının balıkları (durumludur: yüzer, birbirinden kaçar)
    pond: crate::pond::Pond,
    /// Ana menüde kart ve çalar paneli (önceki kareden): balıklar bunların altında soluk çizilir
    home_dim: Vec<Rect>,
}

impl Gui {
    fn new(app: App, fonts: FontFallback) -> Self {
        let tap = app.player.tap();
        Self {
            app,
            start: Instant::now(),
            seek_drag: None,
            vol_drag: None,
            focus_search: false,
            focus_url: false,
            list_offset: 0.0,
            list_view_h: 400.0,
            last_title: String::new(),
            capture: None,
            report_tab: 0,
            help_open: false,
            last_screen: Screen::MainMenu,
            screen_changed: Instant::now(),
            fonts,
            seen_library_gen: u64::MAX,
            seen_log_gen: 0,
            tap,
            spectrum: Spectrum::default(),
            last_spectrum: Instant::now(),
            pond: crate::pond::Pond::default(),
            home_dim: Vec::new(),
        }
    }

    fn go(&mut self, screen: Screen) {
        self.app.screen = screen;
        self.capture = None;
        match screen {
            Screen::Downloader => self.focus_url = true,
            Screen::Library => {
                self.app.scroll_to_selected = true;
            }
            Screen::Report => self.app.refresh_library(),
            _ => {}
        }
    }

    fn activate_menu(&mut self, i: usize) {
        self.app.menu_index = i;
        match i {
            0 => self.go(Screen::Downloader),
            1 => self.go(Screen::Converter),
            2 => self.go(Screen::Library),
            3 => self.go(Screen::Report),
            _ => self.app.should_quit = true,
        }
    }

    // ── Klavye ─────────────────────────────────────────────
    fn handle_keys(&mut self, ctx: &egui::Context) {
        let typing = ctx.wants_keyboard_input();
        let search_focused = ctx.memory(|m| m.has_focus(egui::Id::new(SEARCH_ID)));

        // Fare ile tıklanan düğme klavye odağını tutmasın (Space iki kez tetiklenmesin)
        if !typing
            && let Some(id) = ctx.memory(|m| m.focused())
        {
            ctx.memory_mut(|m| m.surrender_focus(id));
        }

        let events: Vec<KeyEv> = ctx.input(|i| {
            i.events
                .iter()
                .filter_map(|ev| match ev {
                    egui::Event::Key { key, pressed: true, modifiers, .. } => Some(KeyEv::Key(*key, *modifiers)),
                    egui::Event::Text(t) => Some(KeyEv::Text(t.clone())),
                    _ => None,
                })
                .collect()
        });

        // Ayarlarda yeni tuş bekleniyorsa ilk uygun tuş ona gider
        if let Some((action, slot)) = self.capture {
            self.handle_capture(action, slot, &events);
            return;
        }

        if typing {
            // Arama kutusundayken ↑↓ listeyi gezsin (TUI'deki gibi)
            if search_focused {
                for ev in &events {
                    if let KeyEv::Key(k, _) = ev {
                        match k {
                            Key::ArrowUp => self.app.move_selection(-1),
                            Key::ArrowDown => self.app.move_selection(1),
                            Key::PageUp => self.app.move_selection(-15),
                            Key::PageDown => self.app.move_selection(15),
                            _ => {}
                        }
                    }
                }
            }
            return;
        }

        let screen = self.app.screen;

        // İndirici onay soruları açıkken e/h/←→/Enter/Esc sadece soruya gider
        if screen == Screen::Downloader
            && (self.app.pending_playlist.is_some() || self.app.pending_update_offer)
        {
            let is_pl = self.app.pending_playlist.is_some();
            let mut choice = if is_pl { self.app.playlist_choice_yes } else { self.app.update_choice_yes };
            let mut confirm = false;
            let mut cancel = false;
            for ev in &events {
                if let KeyEv::Key(k, _) = ev {
                    match k {
                        Key::ArrowLeft | Key::ArrowRight | Key::Tab => choice = !choice,
                        Key::Enter => confirm = true,
                        Key::Escape => cancel = true,
                        Key::E => choice = true,
                        Key::H => choice = false,
                        _ => {}
                    }
                }
            }
            if is_pl {
                self.app.playlist_choice_yes = choice;
                if cancel {
                    self.app.cancel_playlist();
                } else if confirm {
                    self.app.confirm_playlist_choice(choice);
                }
            } else {
                self.app.update_choice_yes = choice;
                if cancel {
                    self.app.pending_update_offer = false;
                } else if confirm {
                    self.app.answer_update_offer(choice);
                }
            }
            return;
        }

        for ev in events {
            match ev {
                KeyEv::Key(k, m) => {
                    let plain = !m.ctrl && !m.alt && !m.command;
                    if plain && self.nav_key(k) {
                        continue;
                    }
                    // Yazı üretmeyen tuşlar ve Ctrl/Alt'lı kombinasyonlar tuş olayından eşleşir
                    if (keymap::is_non_text(k) || !plain)
                        && let Some(a) = self.app.keymap.match_key(k, m)
                    {
                        self.run_action(a);
                    }
                }
                // Harf/simge kısayolları klavye düzeninden bağımsız olsun diye metin olaylarından okunur
                KeyEv::Text(t) => {
                    for c in t.chars() {
                        if let Some(a) = self.app.keymap.match_char(c) {
                            self.run_action(a);
                        }
                    }
                }
            }
        }
    }

    /// Sabit gezinme tuşları (atanamaz). İşlendiyse `true`.
    fn nav_key(&mut self, k: Key) -> bool {
        if self.help_open && k == Key::Escape {
            self.help_open = false;
            return true;
        }
        match (self.app.screen, k) {
            (Screen::MainMenu, Key::ArrowUp) => self.app.menu_index = self.app.menu_index.saturating_sub(1),
            (Screen::MainMenu, Key::ArrowDown) => self.app.menu_index = (self.app.menu_index + 1).min(MENU_ITEMS - 1),
            (Screen::MainMenu, Key::Enter) => self.activate_menu(self.app.menu_index),
            (Screen::MainMenu, Key::Escape) => self.app.should_quit = true,

            (Screen::Library, Key::ArrowUp) => self.app.move_selection(-1),
            (Screen::Library, Key::ArrowDown) => self.app.move_selection(1),
            (Screen::Library, Key::PageUp) => self.app.move_selection(-15),
            (Screen::Library, Key::PageDown) => self.app.move_selection(15),
            (Screen::Library, Key::Home) => self.app.move_selection(-(isize::MAX / 2)),
            (Screen::Library, Key::End) => self.app.select_last(),
            (Screen::Library, Key::Enter) => self.app.play_selected(),
            (Screen::Library, Key::Escape) => {
                if !self.app.search_query.is_empty() {
                    self.app.search_query.clear();
                    self.app.rebuild_filter();
                } else if self.app.only_favorites {
                    self.app.toggle_only_favorites();
                } else {
                    self.go(Screen::MainMenu);
                }
            }
            (Screen::Downloader, Key::Enter) => self.focus_url = true,
            (Screen::Converter, Key::Enter) => self.app.start_conversion(),
            (_, Key::Escape) => self.go(Screen::MainMenu),
            _ => return false,
        }
        true
    }

    fn handle_capture(&mut self, action: Action, slot: usize, events: &[KeyEv]) {
        for ev in events {
            match ev {
                KeyEv::Key(Key::Escape, _) => {
                    self.capture = None;
                    return;
                }
                KeyEv::Key(Key::Delete, m) if !m.ctrl && !m.alt => {
                    self.app.keymap.clear(action, slot);
                    self.app.mark_dirty();
                    self.app.notify(format!("{}: tus kaldirildi", action.label()));
                    self.capture = None;
                    return;
                }
                KeyEv::Key(k, _) if keymap::is_reserved(*k) => {
                    self.app.notify(format!("{} gezinme icin ayrildi, baska tus sec", k.name()));
                    return;
                }
                KeyEv::Key(k, m) if keymap::is_non_text(*k) || m.ctrl || m.alt || m.command => {
                    let b = Binding::Key { key: *k, ctrl: m.ctrl || m.command, alt: m.alt, shift: m.shift };
                    self.finish_capture(action, slot, b);
                    return;
                }
                KeyEv::Text(t) => {
                    if let Some(c) = t.chars().find(|c| !c.is_whitespace() && !c.is_control()) {
                        self.finish_capture(action, slot, Binding::Char(c));
                        return;
                    }
                }
                _ => {}
            }
        }
    }

    fn finish_capture(&mut self, action: Action, slot: usize, b: Binding) {
        let label = b.label();
        let msg = match self.app.keymap.assign(action, slot, b) {
            Some(other) => format!("{label} > {}  ({} kisayolundan alindi)", action.label(), other.label()),
            None => format!("{label} > {}", action.label()),
        };
        self.app.mark_dirty();
        self.app.notify(msg);
        self.capture = None;
    }

    fn run_action(&mut self, a: Action) {
        let screen = self.app.screen;
        let allowed = match a.scope() {
            Scope::Global => true,
            Scope::Library => screen == Screen::Library,
            Scope::MainMenu => screen == Screen::MainMenu,
            Scope::DownloaderOrSettings => matches!(screen, Screen::Downloader | Screen::Report),
        };
        if !allowed {
            return;
        }
        match a {
            Action::PlayPause => self.app.toggle_pause(),
            Action::Next => self.app.play_next(true),
            Action::Prev => self.app.play_prev(),
            Action::SeekBack => self.app.player.seek_relative(-5_000),
            Action::SeekFwd => self.app.player.seek_relative(5_000),
            Action::SeekBackLong => self.app.player.seek_relative(-30_000),
            Action::SeekFwdLong => self.app.player.seek_relative(30_000),
            Action::VolUp => self.app.adjust_volume(5),
            Action::VolDown => self.app.adjust_volume(-5),
            Action::Mute => self.app.toggle_mute(),
            Action::Stop => self.app.stop(),
            Action::Shuffle => self.app.toggle_shuffle(),
            Action::Loop => self.app.cycle_loop(),
            Action::Sleep => self.app.cycle_sleep(),
            Action::Search => {
                if screen != Screen::Library {
                    self.go(Screen::Library);
                }
                self.focus_search = true;
            }
            Action::Favorite => {
                if let Some(f) = self.app.selected_file() {
                    self.app.toggle_favorite(&f);
                }
            }
            Action::OnlyFavorites => self.app.toggle_only_favorites(),
            Action::Enqueue => {
                if let Some(f) = self.app.selected_file() {
                    self.app.enqueue(&f, false);
                    self.app.move_selection(1);
                }
            }
            Action::PlayNextInQueue => {
                if let Some(f) = self.app.selected_file() {
                    self.app.enqueue(&f, true);
                }
            }
            Action::RevealPlaying => self.app.reveal_playing(),
            Action::Refresh => {
                self.app.refresh_library();
                self.app.notify("Kutuphane yenilendi");
            }
            Action::OpenDir => self.app.open_music_dir(),
            Action::UpdateYtdlp => self.app.start_ytdlp_update(),
            Action::ScreenDownload => self.activate_menu(0),
            Action::ScreenConvert => self.activate_menu(1),
            Action::ScreenLibrary => self.activate_menu(2),
            Action::ScreenSettings => self.activate_menu(3),
            Action::Quit => self.app.should_quit = true,
            Action::Help => self.help_open = !self.help_open,
            Action::ToggleFx => {
                self.app.fx_enabled = !self.app.fx_enabled;
                self.app.mark_dirty();
                self.app.notify(if self.app.fx_enabled { "arka plan efektleri acik" } else { "arka plan efektleri kapali" });
            }
        }
    }

    /// Bir eylemin ilk tuşu (ipuçlarında göstermek için); kapalıysa `None`.
    fn key_hint(&self, a: Action) -> Option<String> {
        let c = self.app.keymap.get(a);
        if !self.app.keymap.enabled || !c.enabled {
            return None;
        }
        c.keys.first().map(Binding::label)
    }

    // ── Ana çizim ──────────────────────────────────────────
    fn draw(&mut self, ui: &mut Ui) {
        let full = ui.max_rect();
        let painter = ui.painter().clone();
        let t = self.start.elapsed().as_secs_f32();

        painter.rect_filled(full, CornerRadius::ZERO, BLACK);
        let home = self.app.screen == Screen::MainMenu;
        let fx = FX_ON.load(Ordering::Relaxed);
        match self.app.theme {
            Theme::Siber => {
                if home && fx {
                    let dt = self.last_spectrum.elapsed().as_secs_f32();
                    self.last_spectrum = Instant::now();
                    let rec = self.tap.recent(2048);
                    self.spectrum.update(rec.as_ref().map(|(v, r)| (v.as_slice(), *r)), dt);
                }
                draw_background(&painter, full, t, home, &self.spectrum.levels);
            }
            // Efektler kapalıyken gölet de durur: düz siyah zemin
            // Yazılar okunsun diye: ana menüde kartın/çalar panelinin altında, diğer ekranlarda her yerde soluk
            Theme::Balik if fx => {
                let dim = if home { self.home_dim.clone() } else { vec![full] };
                self.pond.draw(&painter, full, t, &dim);
            }
            Theme::Balik => {}
        }

        let outer = full.shrink(6.0);
        painter.rect_stroke(outer, CornerRadius::same(10), Stroke::new(1.0_f32, WHITE), StrokeKind::Inside);
        let title = format!(" {STAR_BRIGHT} muzikit {STAR_MID} ");
        let g = painter.layout_no_wrap(title, bold(14.0), WHITE);
        let tpos = pos2(outer.left() + 14.0, outer.top() - g.size().y * 0.5);
        painter.rect_filled(Rect::from_min_size(tpos, g.size()), CornerRadius::ZERO, BLACK);
        painter.galley(tpos, g, WHITE);

        let inner = Rect::from_min_max(
            pos2(outer.left() + 16.0, outer.top() + 18.0),
            pos2(outer.right() - 16.0, outer.bottom() - 14.0),
        );

        match self.app.screen {
            Screen::MainMenu => self.draw_main_menu(ui, inner),
            _ => self.draw_workspace(ui, inner),
        }

        // Bildirim: dış çerçevenin alt kenarına oturur; yumuşakça belirip kaybolur
        if let Some(toast) = &self.app.toast {
            let now = Instant::now();
            let a_in = (now.duration_since(toast.since).as_secs_f32() / 0.18).min(1.0);
            let a_out = (toast.until.saturating_duration_since(now).as_secs_f32() / 0.35).min(1.0);
            let a = a_in * a_out;
            let text = format!(" {STAR_DIM} {} {STAR_DIM} ", toast.text);
            let g = painter.layout_no_wrap(text, mono(13.0), WHITE.gamma_multiply(a));
            let pos = pos2(outer.right() - g.size().x - 18.0, outer.bottom() - g.size().y * 0.5 + (1.0 - a_in) * 8.0);
            painter.rect_filled(Rect::from_min_size(pos, g.size()).expand(2.0), CornerRadius::same(4), BLACK.gamma_multiply(a));
            painter.galley(pos, g, WHITE.gamma_multiply(a));
        }

        let over = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("scanline_overlay")));
        if matches!(self.app.screen, Screen::MainMenu) && FX_ON.load(Ordering::Relaxed) && self.app.theme == Theme::Siber {
            draw_scanlines(&over, full);
        }

        // Ekran geçişi: içerik karanlıktan açılır, üstünden ince bir tarama çizgisi geçer
        if self.app.screen != self.last_screen {
            self.last_screen = self.app.screen;
            self.screen_changed = Instant::now();
        }
        let tp = (self.screen_changed.elapsed().as_secs_f32() / 0.3).min(1.0);
        if tp < 1.0 {
            let clip = over.with_clip_rect(inner.expand(4.0));
            clip.rect_filled(inner.expand(4.0), CornerRadius::ZERO, Color32::from_black_alpha((210.0 * (1.0 - tp).powi(2)) as u8));
            let y = inner.top() + tp * inner.height();
            let a = 1.0 - tp;
            clip.line_segment([pos2(inner.left(), y), pos2(inner.right(), y)], Stroke::new(1.5_f32, WHITE.gamma_multiply(0.7 * a)));
            ui.ctx().request_repaint();
        }

        self.draw_help(ui.ctx(), full);
    }

    /// `?` ile açılan kısayol yardımı: o anki atamaları gösterir.
    fn draw_help(&mut self, ctx: &egui::Context, full: Rect) {
        let a = ctx.animate_bool_with_time(egui::Id::new("help_fade"), self.help_open, 0.15);
        if a <= 0.0 {
            return;
        }
        if self.help_open && ctx.input(|i| i.pointer.any_click()) {
            self.help_open = false;
        }
        let p = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("help_layer")));
        p.rect_filled(full, CornerRadius::ZERO, Color32::from_black_alpha((150.0 * a) as u8));

        let list = self.app.keymap.active_list();
        let half = list.len().div_ceil(2);
        let line_h = 19.0;
        let col_w = 330.0;
        let size = vec2(col_w * 2.0 + 60.0, half as f32 * line_h + 92.0);
        let box_rect = Rect::from_center_size(full.center() + vec2(0.0, (1.0 - a) * 12.0), size);
        p.rect_filled(box_rect, CornerRadius::same(8), Color32::from_black_alpha((252.0 * a) as u8));
        p.rect_stroke(box_rect, CornerRadius::same(8), Stroke::new(1.0_f32, GRAY.gamma_multiply(a)), StrokeKind::Inside);
        corner_brackets(&p, box_rect.shrink(6.0), 10.0, WHITE.gamma_multiply(a));
        p.text(
            pos2(box_rect.center().x, box_rect.top() + 26.0),
            Align2::CENTER_CENTER,
            if self.app.keymap.enabled { "[ kisayollar ]" } else { "[ kisayollar kapali ]" },
            bold(15.0),
            WHITE.gamma_multiply(a),
        );
        for (i, (action, keys)) in list.iter().enumerate() {
            let col = (i / half) as f32;
            let row = (i % half) as f32;
            let x = box_rect.left() + 30.0 + col * (col_w + 10.0);
            let y = box_rect.top() + 56.0 + row * line_h;
            p.text(pos2(x + 120.0, y), Align2::RIGHT_CENTER, keys, mono(12.5), WHITE.gamma_multiply(a));
            p.text(pos2(x + 136.0, y), Align2::LEFT_CENTER, action.label(), mono(12.5), GRAY.gamma_multiply(a));
        }
        p.text(
            pos2(box_rect.center().x, box_rect.bottom() - 18.0),
            Align2::CENTER_CENTER,
            "Esc / tikla: kapat   ·   degistirmek icin: Ayarlar & Kısayollar > kisayollar",
            mono(11.5),
            DARK.gamma_multiply(a),
        );
    }

    // ── Ana menü (TUI'deki ortalanmış kart) ────────────────
    fn draw_main_menu(&mut self, ui: &mut Ui, area: Rect) {
        // Alttaki çalar paneli diğer tüm ekranlardakiyle birebir aynı (aynı boy ve konum)
        let player = Rect::from_min_max(pos2(area.left(), area.bottom() - PLAYER_PANEL_H), area.max);
        let menu_region = Rect::from_min_max(area.min, pos2(area.right(), player.top() - 16.0));

        let items = [
            "1  Muzik indir",
            "2  MP3 -> Opus donustur",
            "3  Kutuphane & Muzik Calar",
            "4  Ayarlar & Kısayollar",
            "5  Cikis",
        ];
        let row_h = 27.0;
        let card_w = 460.0_f32.min(menu_region.width() - 8.0);
        let card_h = (18.0 + 28.0 + 14.0 + items.len() as f32 * row_h + 18.0 + 22.0 + 22.0 + 16.0)
            .min(menu_region.height() - 8.0);
        let card = Rect::from_center_size(menu_region.center(), vec2(card_w, card_h));
        self.home_dim = vec![card.expand(4.0), player.expand(4.0)];

        let p = ui.painter().clone();
        titled_box(&p, card, "", GRAY, WHITE);

        let mut y = card.top() + 18.0;
        p.text(pos2(card.center().x, y + 10.0), Align2::CENTER_CENTER, "[ muzikit ]", bold(16.0), WHITE);
        y += 28.0 + 14.0;

        // Seçim çubuğu bir satırdan diğerine kayarak geçer
        let first_y = y;
        let sel_y = ui.ctx().animate_value_with_time(
            egui::Id::new("menu_sel"),
            first_y + self.app.menu_index as f32 * row_h,
            0.12,
        );
        let sel = Rect::from_min_size(pos2(card.left() + 14.0, sel_y), vec2(card_w - 28.0, row_h - 3.0));
        p.rect_filled(sel, CornerRadius::same(5), SEL);
        p.rect_filled(
            Rect::from_min_size(sel.left_top() + vec2(0.0, 5.0), vec2(2.0, sel.height() - 10.0)),
            CornerRadius::ZERO,
            WHITE,
        );

        for (i, label) in items.iter().enumerate() {
            let r = Rect::from_min_size(pos2(card.left() + 14.0, y), vec2(card_w - 28.0, row_h - 3.0));
            let resp = ui.interact(r, ui.id().with(("menu", i)), Sense::click());
            if resp.hovered() && ui.input(|inp| inp.pointer.delta() != egui::Vec2::ZERO) {
                self.app.menu_index = i;
            }
            let selected = self.app.menu_index == i;
            let (prefix, color, font) = if selected {
                (" > ", WHITE, bold(15.0))
            } else {
                ("   ", DARK, mono(15.0))
            };
            p.text(r.left_center() + vec2(6.0, 0.0), Align2::LEFT_CENTER, format!("{prefix}{label}"), font, color);
            if resp.clicked() {
                self.activate_menu(i);
            }
            y += row_h;
        }
        y += 18.0;

        let count = self.app.songs.len();
        let favs = self.app.favorites.len();
        p.text(
            pos2(card.center().x, y),
            Align2::CENTER_CENTER,
            format!("{STAR_DIM} {count} sarki {STAR_DIM} {favs} favori {STAR_DIM}"),
            mono(13.0),
            GRAY,
        );
        y += 22.0;
        let hint = match self.key_hint(Action::Help) {
            Some(k) => format!("[↑↓] sec [Enter] onayla [{k}] yardim [Esc] cik"),
            None => "[↑↓] sec  [Enter] onayla  [Esc] cik".to_owned(),
        };
        p.text(pos2(card.center().x, y), Align2::CENTER_CENTER, hint, mono(11.5), DARK);

        self.draw_player_panel(ui, player);
    }

    fn now_playing_name(&self) -> String {
        match (&self.app.playing, &self.app.snap.track_name) {
            (Some(f), _) => self.app.display_name(f),
            (None, Some(n)) => crate::utils::strip_control_chars(n),
            _ => "—".to_string(),
        }
    }

    // ── Alt ekranlar: üst sekmeler + içerik + çalar paneli ─
    fn draw_workspace(&mut self, ui: &mut Ui, area: Rect) {
        let header = Rect::from_min_size(area.min, vec2(area.width(), 28.0));
        let player = Rect::from_min_max(pos2(area.left(), area.bottom() - PLAYER_PANEL_H), area.max);
        let content = Rect::from_min_max(
            pos2(area.left(), header.bottom() + 16.0),
            pos2(area.right(), player.top() - 16.0),
        );

        self.draw_tabs(ui, header);

        let p = ui.painter().clone();
        let title = match self.app.screen {
            Screen::Downloader => format!("{STAR_MID} muzik indir"),
            Screen::Converter => format!("{STAR_MID} mp3 -> opus donustur"),
            Screen::Library => format!("{STAR_MID} kutuphane"),
            Screen::Report => format!("{STAR_MID} ayarlar & kısayollar"),
            Screen::MainMenu => String::new(),
        };
        titled_box(&p, content, &title, DARK, WHITE);
        let inner = Rect::from_min_max(content.min + vec2(14.0, 16.0), content.max - vec2(14.0, 12.0));
        ui.scope_builder(UiBuilder::new().max_rect(inner), |ui| match self.app.screen {
            Screen::Downloader => self.draw_downloader(ui),
            Screen::Converter => self.draw_converter(ui),
            Screen::Library => self.draw_library(ui),
            Screen::Report => self.draw_report(ui),
            Screen::MainMenu => {}
        });

        self.draw_player_panel(ui, player);
    }

    fn draw_tabs(&mut self, ui: &mut Ui, rect: Rect) {
        let p = ui.painter().clone();
        p.rect_filled(rect.expand2(vec2(4.0, 2.0)), CornerRadius::same(6), panel_bg());
        ui.scope_builder(UiBuilder::new().max_rect(rect), |ui| {
            ui.horizontal_centered(|ui| {
                let tabs = [
                    (Screen::Downloader, "[1] indir"),
                    (Screen::Converter, "[2] donustur"),
                    (Screen::Library, "[3] kutuphane"),
                    (Screen::Report, "[4] ayarlar & kısayollar"),
                ];
                if tbtn(ui, "‹ menu").on_hover_text("Esc").clicked() {
                    self.go(Screen::MainMenu);
                }
                ui.label(RichText::new("│").color(DIM));
                for (screen, label) in tabs {
                    let on = self.app.screen == screen;
                    if tbtn_on(ui, label, on).clicked() {
                        self.go(screen);
                    }
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if let Some(since) = self.app.busy_since {
                        let frame = (self.start.elapsed().as_millis() / 250) as usize % SPINNER.len();
                        let what = if self.app.is_downloading {
                            "indiriliyor"
                        } else if self.app.is_updating {
                            "yt-dlp guncelleniyor"
                        } else {
                            "donusturuluyor"
                        };
                        ui.label(
                            RichText::new(format!(
                                "{} {what} {}",
                                SPINNER[frame],
                                format_ms(since.elapsed().as_millis() as u64)
                            ))
                            .color(WHITE),
                        );
                    } else {
                        ui.label(RichText::new(format!("{STAR_DIM} {} sarki", self.app.songs.len())).color(DARK));
                    }
                });
            });
        });
    }

    // ── Çalar paneli (TUI'deki "pipewire calar") ───────────
    fn transport_buttons(&mut self, ui: &mut Ui) {
        let status = self.app.snap.status;
        if tbtn(ui, "⏮").on_hover_text("Onceki [b]").clicked() {
            self.app.play_prev();
        }
        let pp = if status == PlaybackStatus::Playing { "⏸" } else { "▶" };
        let r = ui.add(egui::Button::new(RichText::new(pp).color(WHITE)).stroke(Stroke::new(1.0_f32, GRAY)));
        if r.on_hover_text("Cal / Duraklat [Space]").clicked() {
            self.app.toggle_pause();
        }
        if tbtn(ui, "⏭").on_hover_text("Sonraki [n]").clicked() {
            self.app.play_next(true);
        }
        if tbtn(ui, "■").on_hover_text("Durdur [x]").clicked() {
            self.app.stop();
        }
    }

    fn draw_player_panel(&mut self, ui: &mut Ui, rect: Rect) {
        let p = ui.painter().clone();
        let snap = self.app.snap.clone();
        let playing = snap.status == PlaybackStatus::Playing;
        titled_box(
            &p,
            rect,
            &format!("{STAR_BRIGHT} pipewire calar {STAR_MID}"),
            if playing { WHITE } else { DARK },
            WHITE,
        );
        let inner = Rect::from_min_max(rect.min + vec2(14.0, 14.0), rect.max - vec2(14.0, 10.0));

        ui.scope_builder(UiBuilder::new().max_rect(inner), |ui| {
            ui.spacing_mut().item_spacing.y = 7.0;

            // Satır 1: durum + şarkı adı + favori
            ui.horizontal(|ui| {
                let status = match snap.status {
                    PlaybackStatus::Playing => "▶ CALIYOR",
                    PlaybackStatus::Paused => "⏸ DURAKLATILDI",
                    PlaybackStatus::Idle => "■ BEKLEMEDE",
                };
                ui.label(RichText::new(status).font(bold(14.0)).color(WHITE));
                ui.add_space(6.0);
                if let Some(err) = &snap.last_error {
                    ui.add(egui::Label::new(RichText::new(format!("! {err}")).color(WHITE)).truncate());
                } else if self.app.playing.is_some() || snap.track_name.is_some() {
                    let fav_file = self.app.playing.clone();
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if let Some(f) = &fav_file {
                            let on = self.app.is_favorite(f);
                            let star = if on { "★" } else { "☆" };
                            if ui
                                .add(egui::Button::new(RichText::new(star).color(if on { WHITE } else { DARK })).frame(false))
                                .on_hover_text("Favori [f]")
                                .clicked()
                            {
                                self.app.toggle_favorite(f);
                            }
                        }
                        ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                            let name = self.now_playing_name();
                            let r = ui.add(
                                egui::Label::new(RichText::new(name).color(GRAY))
                                    .truncate()
                                    .sense(Sense::click()),
                            );
                            if r.on_hover_text("Kutuphanede goster").clicked() {
                                self.go(Screen::Library);
                                self.app.reveal_playing();
                            }
                        });
                    });
                } else {
                    ui.label(RichText::new("bir sarki sec  [Enter]/cift tikla cal").color(DARK));
                }
            });

            // Satır 2: ilerleme çubuğu + süre
            ui.horizontal(|ui| {
                let dur = snap.duration_ms;
                let frac = if dur > 0 { snap.position_ms as f32 / dur as f32 } else { 0.0 };
                let w = ui.available_width() - 130.0;
                let out = progress_bar(ui, w, frac, &mut self.seek_drag, dur > 0, |f| {
                    format_ms((f as f64 * dur as f64) as u64)
                });
                if let Some(f) = out.committed {
                    self.app.player.seek_to((f as f64 * dur as f64) as u64);
                }
                let shown_pos = match self.seek_drag {
                    Some(f) => (f as f64 * dur as f64) as u64,
                    None => snap.position_ms,
                };
                ui.label(
                    RichText::new(format!("{} / {}", format_ms(shown_pos), format_ms(dur))).color(GRAY),
                );
            });

            // Satır 3: kontroller
            ui.horizontal(|ui| {
                self.transport_buttons(ui);
                ui.label(RichText::new("│").color(DIM));

                if tbtn_on(ui, &format!("karistir: {}", if self.app.shuffle { "acik" } else { "kapali" }), self.app.shuffle)
                    .on_hover_text("[s]")
                    .clicked()
                {
                    self.app.toggle_shuffle();
                }
                if tbtn_on(ui, &format!("dongu: {}", self.app.loop_mode.label()), self.app.loop_mode != LoopMode::Off)
                    .on_hover_text("[d] Kapali → Liste → Tek")
                    .clicked()
                {
                    self.app.cycle_loop();
                }
                let sleep_on = self.app.sleep != SleepTimer::Off;
                if tbtn_on(ui, &format!("uyku: {}", self.app.sleep_label()), sleep_on)
                    .on_hover_text("[t] 15/30/45/60/90 dk veya sarki sonu")
                    .clicked()
                {
                    self.app.cycle_sleep();
                }
                if !self.app.queue.is_empty() {
                    ui.label(RichText::new(format!("sira: {}", self.app.queue.len())).color(GRAY));
                }

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let vol = self.app.snap.volume_pct;
                    ui.label(RichText::new(format!("%{vol:<3}")).color(GRAY));
                    let out = progress_bar(ui, 110.0, vol as f32 / 150.0, &mut self.vol_drag, true, |f| {
                        format!("%{}", (f * 150.0).round() as u32)
                    });
                    if let Some(f) = out.live.or(out.committed) {
                        self.app.set_volume((f * 150.0).round() as u32);
                    }
                    let muted = self.app.muted_from.is_some();
                    if tbtn_on(ui, if muted { "ses: kapali" } else { "ses" }, !muted)
                        .on_hover_text("[m] sessiz  [+/-] %5")
                        .clicked()
                    {
                        self.app.toggle_mute();
                    }
                });
            });
        });
    }

    // ── Kütüphane ──────────────────────────────────────────
    fn draw_library(&mut self, ui: &mut Ui) {
        // Araç çubuğu
        ui.horizontal(|ui| {
            ui.label(RichText::new("/").color(WHITE));
            let resp = ui.add(
                egui::TextEdit::singleline(&mut self.app.search_query)
                    .id(egui::Id::new(SEARCH_ID))
                    .hint_text("ara... (birden fazla kelime)")
                    .desired_width(260.0)
                    .char_limit(128),
            );
            if self.focus_search {
                resp.request_focus();
                self.focus_search = false;
            }
            if resp.changed() {
                self.app.rebuild_filter();
            }
            if resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                self.app.play_selected();
            }
            if resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Escape)) {
                self.app.search_query.clear();
                self.app.rebuild_filter();
            }

            let mut new_sort = None;
            egui::ComboBox::from_id_salt("sort")
                .selected_text(self.app.sort_mode.label())
                .width(110.0)
                .show_ui(ui, |ui| {
                    for m in [SortMode::Number, SortMode::NumberDesc, SortMode::Name] {
                        if ui.selectable_label(self.app.sort_mode == m, m.label()).clicked() {
                            new_sort = Some(m);
                        }
                    }
                });
            if let Some(m) = new_sort {
                self.app.set_sort(m);
            }
            if tbtn_on(ui, "★ favoriler", self.app.only_favorites).on_hover_text("[F]").clicked() {
                self.app.toggle_only_favorites();
            }
            if tbtn(ui, "↻").on_hover_text("Yenile [r]").clicked() {
                self.app.refresh_library();
                self.app.notify("Kutuphane yenilendi");
            }
            if tbtn(ui, "◎").on_hover_text("Calani goster [c]").clicked() {
                self.app.reveal_playing();
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(
                    RichText::new(format!("{}/{}", self.app.filtered.len(), self.app.songs.len())).color(DARK),
                );
            });
        });
        ui.add_space(6.0);

        let area = ui.available_rect_before_wrap();
        let show_side = area.width() > 760.0;
        let side_w = if show_side { 300.0 } else { 0.0 };
        let list_rect = Rect::from_min_max(area.min, pos2(area.right() - side_w - if show_side { 14.0 } else { 0.0 }, area.bottom()));
        let side_rect = Rect::from_min_max(pos2(area.right() - side_w, area.top()), area.max);

        let mut actions: Vec<RowAction> = Vec::new();
        ui.scope_builder(UiBuilder::new().max_rect(list_rect), |ui| {
            self.draw_song_list(ui, &mut actions);
        });
        if show_side {
            ui.scope_builder(UiBuilder::new().max_rect(side_rect), |ui| {
                self.draw_library_side(ui, &mut actions);
            });
        }
        ui.allocate_rect(area, Sense::hover());

        for a in actions {
            match a {
                RowAction::Select(row) => {
                    self.app.selected = row;
                }
                RowAction::Play(f) => self.app.play_file(&f, true),
                RowAction::Enqueue(f, front) => self.app.enqueue(&f, front),
                RowAction::Fav(f) => self.app.toggle_favorite(&f),
                RowAction::Copy(f) => {
                    ui.ctx().copy_text(f);
                    self.app.notify("Dosya adi kopyalandi");
                }
                RowAction::OpenDir => self.app.open_music_dir(),
            }
        }
    }

    fn draw_song_list(&mut self, ui: &mut Ui, actions: &mut Vec<RowAction>) {
        let n = self.app.filtered.len();
        if n == 0 {
            ui.add_space(20.0);
            let msg = if self.app.songs.is_empty() {
                format!("Bu klasorde .opus dosyasi yok:\n{}", self.app.music_dir.display())
            } else {
                "Aramaya uyan sarki yok.  [Esc] temizle".to_string()
            };
            ui.label(RichText::new(msg).color(DARK));
            return;
        }

        ui.spacing_mut().item_spacing.y = 0.0;
        let view_h = ui.available_height();
        self.list_view_h = view_h;

        let mut sa = egui::ScrollArea::vertical()
            .id_salt("songs")
            .auto_shrink([false, false]);
        if self.app.scroll_to_selected {
            self.app.scroll_to_selected = false;
            let top = self.app.selected as f32 * ROW_H;
            if top < self.list_offset || top + ROW_H > self.list_offset + view_h {
                sa = sa.vertical_scroll_offset((top - view_h * 0.4).max(0.0));
            }
        }

        let selected = self.app.selected;
        let playing = self.app.playing.clone();
        let app = &self.app;
        let out = sa.show_rows(ui, ROW_H, n, |ui, range| {
            for row in range {
                let song = &app.songs[app.filtered[row]];
                let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::click());
                let p = ui.painter();
                let is_sel = row == selected;
                let is_play = playing.as_deref() == Some(song.file.as_str());
                let is_fav = app.is_favorite(&song.file);

                if is_sel {
                    p.rect_filled(rect.shrink2(vec2(0.0, 1.0)), CornerRadius::same(4), SEL);
                } else if resp.hovered() {
                    p.rect_filled(rect.shrink2(vec2(0.0, 1.0)), CornerRadius::same(4), HOVER);
                }
                let prefix = match (is_sel, is_play) {
                    (true, true) => ">♪",
                    (true, false) => "> ",
                    (false, true) => " ♪",
                    _ => "  ",
                };
                let y = rect.center().y;
                let x0 = rect.left() + 6.0;
                p.text(pos2(x0, y), Align2::LEFT_CENTER, prefix, mono(14.0), WHITE);
                let num = song.number.map(|n| format!("{n:04}")).unwrap_or_else(|| "----".into());
                p.text(pos2(x0 + 26.0, y), Align2::LEFT_CENTER, num, mono(13.0), if is_sel { GRAY } else { DARK });
                let color = if is_sel || is_play { WHITE } else { GRAY };
                let font = if is_play { bold(14.0) } else { mono(14.0) };
                let title_x = x0 + 74.0;
                text_trunc(p, pos2(title_x, y), &song.title, font, color, rect.right() - title_x - 30.0);

                let star_rect = Rect::from_center_size(pos2(rect.right() - 14.0, y), vec2(20.0, ROW_H));
                let star_hover = resp.hover_pos().is_some_and(|hp| star_rect.contains(hp));
                if is_fav || star_hover {
                    p.text(
                        star_rect.center(),
                        Align2::CENTER_CENTER,
                        if is_fav { "★" } else { "☆" },
                        mono(14.0),
                        if is_fav { WHITE } else { DARK },
                    );
                }

                if resp.clicked() {
                    if star_hover {
                        actions.push(RowAction::Fav(song.file.clone()));
                    } else {
                        actions.push(RowAction::Select(row));
                    }
                }
                if resp.double_clicked() && !star_hover {
                    actions.push(RowAction::Play(song.file.clone()));
                }
                if resp.secondary_clicked() {
                    actions.push(RowAction::Select(row));
                }
                let file = song.file.clone();
                resp.context_menu(|ui| {
                    ui.set_min_width(190.0);
                    if ui.button("▶  cal").clicked() {
                        actions.push(RowAction::Play(file.clone()));
                        ui.close();
                    }
                    if ui.button("⤴  siradaki olarak cal").clicked() {
                        actions.push(RowAction::Enqueue(file.clone(), true));
                        ui.close();
                    }
                    if ui.button("+  siraya ekle").clicked() {
                        actions.push(RowAction::Enqueue(file.clone(), false));
                        ui.close();
                    }
                    ui.separator();
                    let fav_label = if is_fav { "☆  favoriden cikar" } else { "★  favorilere ekle" };
                    if ui.button(fav_label).clicked() {
                        actions.push(RowAction::Fav(file.clone()));
                        ui.close();
                    }
                    if ui.button("⧉  dosya adini kopyala").clicked() {
                        actions.push(RowAction::Copy(file.clone()));
                        ui.close();
                    }
                    if ui.button("📂  klasoru ac").clicked() {
                        actions.push(RowAction::OpenDir);
                        ui.close();
                    }
                });
            }
        });
        self.list_offset = out.state.offset.y;
    }

    fn draw_library_side(&mut self, ui: &mut Ui, actions: &mut Vec<RowAction>) {
        let area = ui.max_rect();
        let p = ui.painter().clone();
        let queue_h = (area.height() * 0.45).max(120.0);
        let info_h = 112.0;
        let q_rect = Rect::from_min_size(area.min + vec2(0.0, 8.0), vec2(area.width(), queue_h - 8.0));
        let info_rect = Rect::from_min_max(pos2(area.left(), area.bottom() - info_h), area.max);
        let recent_rect = Rect::from_min_max(pos2(area.left(), q_rect.bottom() + 18.0), pos2(area.right(), info_rect.top() - 18.0));

        // Sıradaki
        titled_box(&p, q_rect, &format!("{STAR_MID} siradaki ({})", self.app.queue.len()), DIM, GRAY);
        let mut q_action: Option<(usize, i8)> = None; // -1 yukari, 1 asagi, 0 sil
        let mut clear = false;
        ui.scope_builder(UiBuilder::new().max_rect(q_rect.shrink2(vec2(10.0, 12.0))), |ui| {
            if self.app.queue.is_empty() {
                ui.label(RichText::new("bos — sag tik › siraya ekle\nveya [a] / [A] tusu").color(DARK).small());
                return;
            }
            egui::ScrollArea::vertical().id_salt("queue").auto_shrink([false, false]).max_height(ui.available_height() - 28.0).show(ui, |ui| {
                for (i, f) in self.app.queue.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui.small_button("✕").clicked() {
                                q_action = Some((i, 0));
                            }
                            if ui.small_button("↓").clicked() {
                                q_action = Some((i, 1));
                            }
                            if ui.small_button("↑").clicked() {
                                q_action = Some((i, -1));
                            }
                            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(format!("{:>2}. {}", i + 1, self.app.display_name(f))).color(GRAY).small(),
                                    )
                                    .truncate(),
                                );
                            });
                        });
                    });
                }
            });
            if tbtn(ui, "temizle").clicked() {
                clear = true;
            }
        });
        if clear {
            self.app.queue.clear();
        }
        if let Some((i, op)) = q_action {
            match op {
                0 => {
                    self.app.queue.remove(i);
                }
                -1 => self.app.queue_move(i, true),
                _ => self.app.queue_move(i, false),
            }
        }

        // Son çalınanlar
        if recent_rect.height() > 60.0 {
            titled_box(&p, recent_rect, &format!("{STAR_MID} son calinanlar"), DIM, GRAY);
            ui.scope_builder(UiBuilder::new().max_rect(recent_rect.shrink2(vec2(10.0, 12.0))), |ui| {
                egui::ScrollArea::vertical().id_salt("recent").auto_shrink([false, false]).show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    for f in self.app.recent.iter() {
                        let r = ui.add(
                            egui::Label::new(RichText::new(self.app.display_name(f)).color(GRAY).small())
                                .truncate()
                                .sense(Sense::click()),
                        );
                        if r.on_hover_text("cift tikla: cal").double_clicked() {
                            actions.push(RowAction::Play(f.clone()));
                        }
                    }
                    if self.app.recent.is_empty() {
                        ui.label(RichText::new("henuz yok").color(DARK).small());
                    }
                });
            });
        }

        // Bilgi
        titled_box(&p, info_rect, &format!("{STAR_MID} bilgi"), DIM, GRAY);
        ui.scope_builder(UiBuilder::new().max_rect(info_rect.shrink2(vec2(12.0, 12.0))), |ui| {
            ui.spacing_mut().item_spacing.y = 1.0;
            if let Some(s) = &self.app.library_stats {
                ui.label(RichText::new(format!("opus dosyasi : {}", s.total_opus_files)).color(GRAY).small());
                ui.label(RichText::new(format!("son numara   : {:04}", s.max_number)).color(GRAY).small());
                ui.label(RichText::new(format!("log kaydi    : {}", s.downloaded_txt_count)).color(GRAY).small());
                let miss = if s.missing_numbers.is_empty() {
                    "eksik yok ✓".to_string()
                } else {
                    format!("{} eksik numara", s.missing_numbers.len())
                };
                ui.label(RichText::new(format!("durum        : {miss}")).color(if s.missing_numbers.is_empty() { GRAY } else { WHITE }).small());
            }
            ui.label(RichText::new(format!("favori       : {}", self.app.favorites.len())).color(GRAY).small());
        });
    }

    // ── İndirici ───────────────────────────────────────────
    fn draw_downloader(&mut self, ui: &mut Ui) {
        ui.label(RichText::new("youtube / youtube music linki — birden fazla link icin aralarina bosluk birak").color(DARK).small());
        // Butonlar önce sağa yerleşir, link kutusu kalan alanı doldurur: pencere ne kadar
        // daralırsa daralsın butonlar taşmaz
        ui.horizontal(|ui| {
            ui.label(RichText::new(">").color(WHITE));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let upd = ui
                    .add_enabled(!self.app.is_downloading && !self.app.is_updating, egui::Button::new(RichText::new("[u] guncelle").color(GRAY)))
                    .on_hover_text("yt-dlp'yi guncelle");
                let can = !self.app.url_input.trim().is_empty();
                let btn = ui.add_enabled(can, egui::Button::new(RichText::new("[Enter] indir").color(WHITE)).stroke(Stroke::new(1.0_f32, GRAY)));
                let resp = ui.add_sized(
                    vec2(ui.available_width().max(80.0), ui.spacing().interact_size.y),
                    egui::TextEdit::singleline(&mut self.app.url_input)
                        .id(egui::Id::new(URL_ID))
                        .hint_text("https://www.youtube.com/watch?v=...")
                        .char_limit(crate::utils::MAX_URL_LEN * 8),
                );
                if self.focus_url {
                    resp.request_focus();
                    self.focus_url = false;
                }
                let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
                if (enter || btn.clicked()) && can {
                    self.app.submit_urls();
                }
                if upd.clicked() {
                    self.app.start_ytdlp_update();
                }
            });
        });

        // Durum satırı
        ui.horizontal(|ui| {
            let status = if self.app.is_downloading {
                "durum: indiriliyor".to_string()
            } else if self.app.is_updating {
                "durum: yt-dlp guncelleniyor".to_string()
            } else {
                "durum: hazir".to_string()
            };
            ui.label(RichText::new(status).color(if self.app.is_busy() { WHITE } else { DARK }).small());
            if !self.app.url_queue.is_empty() {
                ui.label(RichText::new(format!("{STAR_DIM} kuyrukta {} link", self.app.url_queue.len())).color(GRAY).small());
                if ui.small_button("kuyrugu bosalt").clicked() {
                    self.app.url_queue.clear();
                }
            }
            if let Some(v) = &self.app.ytdlp_version {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(RichText::new(format!("yt-dlp {v}")).color(DARK).small());
                });
            }
        });

        // Onay soruları
        if self.app.pending_playlist.is_some() || self.app.pending_update_offer {
            ui.add_space(6.0);
            let is_pl = self.app.pending_playlist.is_some();
            let (prompt, yes) = if is_pl {
                ("Playlist algilandi. Tum playlist indirilsin mi?", self.app.playlist_choice_yes)
            } else {
                ("Indirmede hata oldu. yt-dlp guncellensin mi?", self.app.update_choice_yes)
            };
            egui::Frame::new()
                .stroke(Stroke::new(1.0_f32, WHITE))
                .corner_radius(CornerRadius::same(6))
                .inner_margin(egui::Margin::same(8))
                .show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(prompt).color(WHITE));
                        let y = tbtn_on(ui, "[e] Evet", yes);
                        let n = tbtn_on(ui, "[h] Hayir (Varsayilan)", !yes);
                        ui.label(RichText::new("[Enter] onayla  [Esc] iptal").color(DARK).small());
                        let mut answer = None;
                        if y.clicked() {
                            answer = Some(true);
                        }
                        if n.clicked() {
                            answer = Some(false);
                        }
                        if let Some(a) = answer {
                            if is_pl {
                                self.app.confirm_playlist_choice(a);
                            } else {
                                self.app.answer_update_offer(a);
                            }
                        }
                    });
                });
        }

        ui.add_space(8.0);
        self.draw_log(ui, "log");
    }

    fn draw_log(&mut self, ui: &mut Ui, title: &str) {
        let rect = ui.available_rect_before_wrap();
        let p = ui.painter().clone();
        let box_rect = Rect::from_min_max(rect.min + vec2(0.0, 8.0), rect.max);
        titled_box(&p, box_rect, &format!("{STAR_DIM} {title}"), DIM, GRAY);
        ui.scope_builder(UiBuilder::new().max_rect(box_rect.shrink2(vec2(10.0, 12.0))), |ui| {
            ui.horizontal(|ui| {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.small_button("temizle").clicked() {
                        self.app.logs.clear();
                    }
                    if ui.small_button("kopyala").clicked() {
                        ui.ctx().copy_text(self.app.logs.join("\n"));
                        self.app.notify("Log kopyalandi");
                    }
                    if ui.small_button("klasoru ac").clicked() {
                        self.app.open_music_dir();
                    }
                });
            });
            egui::ScrollArea::vertical()
                .id_salt("log")
                .auto_shrink([false, false])
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    if self.app.logs.is_empty() {
                        ui.label(RichText::new("henuz kayit yok").color(DARK).small());
                    }
                    for line in &self.app.logs {
                        let lower = line.to_lowercase();
                        let color = if lower.contains("hata") || lower.contains("basarisiz") || lower.contains("guvenlik") {
                            WHITE
                        } else {
                            GRAY
                        };
                        ui.add(egui::Label::new(RichText::new(line).color(color).small()).wrap());
                    }
                });
        });
        ui.allocate_rect(rect, Sense::hover());
    }

    // ── Dönüştürücü ────────────────────────────────────────
    fn draw_converter(&mut self, ui: &mut Ui) {
        egui::Grid::new("conv").num_columns(2).spacing(vec2(12.0, 8.0)).show(ui, |ui| {
            ui.label(RichText::new("kaynak (mp3)").color(GRAY));
            ui.add(egui::TextEdit::singleline(&mut self.app.conv_input).desired_width(ui.available_width().min(560.0)));
            ui.end_row();

            ui.label(RichText::new("hedef (opus)").color(GRAY));
            ui.add(egui::TextEdit::singleline(&mut self.app.conv_output).desired_width(ui.available_width().min(560.0)));
            ui.end_row();

            ui.label(RichText::new("bitrate").color(GRAY));
            ui.horizontal(|ui| {
                for b in ["64k", "96k", "128k", "160k", "192k", "256k"] {
                    if tbtn_on(ui, b, self.app.conv_bitrate == b).clicked() {
                        self.app.conv_bitrate = b.to_string();
                    }
                }
                ui.add(egui::TextEdit::singleline(&mut self.app.conv_bitrate).desired_width(60.0).char_limit(5));
            });
            ui.end_row();
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let btn = ui.add_enabled(
                !self.app.is_converting,
                egui::Button::new(RichText::new("[Enter] donusturmeyi baslat").color(WHITE)).stroke(Stroke::new(1.0_f32, GRAY)),
            );
            if btn.clicked() {
                self.app.start_conversion();
            }
            ui.label(
                RichText::new("filtreler: highpass=20, lowpass=20000, VOIP modu (nm2.py ile ayni)")
                    .color(DARK)
                    .small(),
            );
        });
        ui.add_space(6.0);
        self.draw_log(ui, "log");
    }

    // ── Ayarlar & Kısayollar ───────────────────────────────
    fn draw_report(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            for (i, name) in ["ayarlar", "kisayollar", "esitle"].into_iter().enumerate() {
                if tbtn_on(ui, name, self.report_tab == i as u8).clicked() {
                    self.report_tab = i as u8;
                    self.capture = None;
                }
            }
        });
        ui.add_space(8.0);
        match self.report_tab {
            0 => self.draw_settings(ui),
            1 => self.draw_shortcuts(ui),
            _ => self.draw_sync(ui),
        }
    }

    /// Telefonla eşitleme sekmesi: bul → bağlan/eşleş → planı gör → onayla.
    fn draw_sync(&mut self, ui: &mut Ui) {
        use crate::sync::Stage;
        let app = &mut self.app;
        let busy_other = app.is_downloading || app.is_converting || app.is_updating;
        egui::ScrollArea::vertical().id_salt("sync").auto_shrink([false, false]).show(ui, |ui| {
            ui.label(RichText::new(format!("{STAR_MID} telefonla esitle")).font(bold(14.0)).color(WHITE));
            ui.label(
                RichText::new("ayni Wi-Fi'da, telefonda muzikit > ayarlar > bilgisayarla esitle ekrani acikken calisir. numaralar degismez; sadece eksik sarkilar karsiya gider.")
                    .color(DARK)
                    .small(),
            );
            ui.add_space(8.0);

            let idle = !app.sync.busy();
            let mut go: Option<(std::net::SocketAddr, Option<String>)> = None;
            ui.horizontal(|ui| {
                if ui.add_enabled(idle && !busy_other, egui::Button::new(RichText::new("[ara] telefonu bul").color(GRAY))).clicked() {
                    app.sync.search();
                }
                ui.label(RichText::new("ya da IP").color(DARK));
                ui.add(egui::TextEdit::singleline(&mut app.sync.manual).hint_text("192.168.1.20").desired_width(160.0));
                if ui.add_enabled(idle && !busy_other, egui::Button::new(RichText::new("baglan").color(GRAY))).clicked() {
                    match crate::sync::proto::parse_manual(&app.sync.manual) {
                        Ok(a) => go = Some((a, None)),
                        Err(e) => app.sync.stage = Stage::Failed(e.to_string()),
                    }
                }
            });
            if busy_other {
                ui.label(RichText::new("indirme/donusturme bitince esitleyebilirsin").color(DARK).small());
            }
            for peer in app.sync.peers.clone() {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("  {}  {}", peer.name, peer.addr)).color(GRAY));
                    if ui.add_enabled(idle && !busy_other, egui::Button::new(RichText::new("baglan").color(GRAY))).clicked() {
                        go = Some((peer.addr, None));
                    }
                });
            }
            ui.add_space(8.0);

            let mut approve = false;
            let mut cancel = false;
            match &app.sync.stage {
                Stage::Idle => {}
                Stage::Searching => {
                    ui.label(RichText::new("telefon araniyor...").color(GRAY));
                }
                Stage::Working => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(RichText::new("calisiyor, pencereyi kapatma").color(GRAY));
                    });
                }
                Stage::NeedCode { addr, phone_name, pairing } => {
                    let addr = *addr;
                    ui.label(RichText::new(format!("{phone_name} ile ilk eslestirme")).color(WHITE));
                    if !*pairing {
                        ui.label(RichText::new("telefonda eslestirme kodu acik degil: telefondaki esitleme ekraninda kodu goster").color(DARK).small());
                    }
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("telefondaki kod").color(DARK));
                        ui.add(egui::TextEdit::singleline(&mut app.sync.code).hint_text("XXXX-XXXX-XXXX").desired_width(160.0).char_limit(16));
                        if tbtn(ui, "eslestir").clicked() {
                            go = Some((addr, Some(app.sync.code.clone())));
                        }
                        if tbtn(ui, "vazgec").clicked() {
                            cancel = true;
                        }
                    });
                }
                Stage::Ready(p) => {
                    ui.label(RichText::new(format!("{} ile esitleme plani", p.phone_name)).color(WHITE));
                    let rows = [
                        ("bilgisayara gelecek", p.to_pc().to_string()),
                        ("telefona gidecek", p.to_phone().to_string()),
                        ("numarasi kayacak", p.plan.renames.len().to_string()),
                        ("downloaded.txt'ye eklenecek", format!("pc +{}, telefon +{}", p.new_ids.0, p.new_ids.1)),
                    ];
                    egui::Grid::new("sync_plan").num_columns(2).spacing(vec2(16.0, 4.0)).show(ui, |ui| {
                        for (k, v) in rows {
                            ui.label(RichText::new(k).color(DARK));
                            ui.label(RichText::new(v).color(GRAY));
                            ui.end_row();
                        }
                    });
                    let mut detail: Vec<String> = Vec::new();
                    for t in &p.plan.transfers {
                        let arrow = if t.to == crate::sync::algo::Side::L { "<- pc" } else { "-> telefon" };
                        detail.push(format!("{arrow}  {}", t.name));
                    }
                    for r in &p.plan.renames {
                        let side = if r.side == crate::sync::algo::Side::L { "pc" } else { "telefon" };
                        detail.push(format!("kayma ({side})  {:04} -> {}", r.from, r.name));
                    }
                    for m in &p.plan.mismatch {
                        detail.push(format!("bilgi: ayni sarki farkli numarada (pc {:04}, telefon {:04})", m.l, m.r));
                    }
                    for d in &p.plan.duplicates {
                        let side = if d.side == crate::sync::algo::Side::L { "pc" } else { "telefon" };
                        detail.push(format!("bilgi: {side} tarafinda {:04} bir kopya, gonderilmeyecek", d.number));
                    }
                    for line in detail.iter().take(40) {
                        ui.label(RichText::new(line).color(GRAY).small());
                    }
                    if detail.len() > 40 {
                        ui.label(RichText::new(format!("... ve {} satir daha", detail.len() - 40)).color(DARK).small());
                    }
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        if tbtn(ui, "[onayla] esitle").clicked() {
                            approve = true;
                        }
                        if tbtn(ui, "vazgec").clicked() {
                            cancel = true;
                        }
                    });
                }
                Stage::Done(msg) => {
                    ui.label(RichText::new(msg).color(WHITE));
                }
                Stage::Failed(msg) => {
                    ui.label(RichText::new(format!("hata: {msg}")).color(GRAY));
                }
            }
            if approve {
                app.sync.approve();
            }
            if cancel {
                app.sync.cancel();
            }
            if let Some((addr, code)) = go {
                let (dir, log) = (app.music_dir.clone(), app.log_file.clone());
                app.sync.connect(addr, code, dir, log);
            }

            if !app.sync.log.is_empty() {
                ui.add_space(8.0);
                for l in app.sync.log.iter().rev().take(12).rev() {
                    ui.label(RichText::new(l).color(DARK).small());
                }
            }

            ui.add_space(14.0);
            ui.separator();
            ui.label(RichText::new("baglanti sifreli (AES-256-GCM) ve imzali; sadece yerel agda calisir").color(DARK).small());
            if app.sync.confirm_forget {
                ui.label(
                    RichText::new("eslesme anahtarlari ve bu bilgisayarin esitleme kimligi silinecek; sonraki esitlemede telefondaki kod yeniden istenir. muzikler, downloaded.txt ve defter silinmez.")
                        .color(GRAY)
                        .small(),
                );
                ui.horizontal(|ui| {
                    if tbtn(ui, "[evet] gecmisi sil").clicked() {
                        app.sync.forget();
                    }
                    if tbtn(ui, "vazgec").clicked() {
                        app.sync.confirm_forget = false;
                    }
                });
            } else if ui.add_enabled(idle, egui::Button::new(RichText::new("esitleme gecmisini sil").color(DARK))).clicked() {
                app.sync.confirm_forget = true;
            }
        });
        if self.app.sync.busy() {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(150));
        }
    }

    /// Kütüphane sağlık raporu (ayarlar sekmesinin en altında).
    fn draw_report_stats(&mut self, ui: &mut Ui) {
        ui.label(RichText::new(format!("{STAR_MID} kutuphane saglik raporu")).font(bold(14.0)).color(WHITE));
        ui.add_space(4.0);
        if let Some(s) = &self.app.library_stats {
            egui::Grid::new("stats").num_columns(2).spacing(vec2(16.0, 4.0)).show(ui, |ui| {
                let rows = [
                    ("taranan dizin", self.app.music_dir.display().to_string()),
                    ("opus dosyasi", s.total_opus_files.to_string()),
                    ("en yuksek numara", format!("{:04}", s.max_number)),
                    ("downloaded.txt kaydi", s.downloaded_txt_count.to_string()),
                    ("favori", self.app.favorites.len().to_string()),
                ];
                for (k, v) in rows {
                    ui.label(RichText::new(k).color(DARK));
                    ui.label(RichText::new(v).color(GRAY));
                    ui.end_row();
                }
            });
            ui.add_space(4.0);
            if s.missing_numbers.is_empty() {
                ui.label(RichText::new("✓ numaralandirmada eksik/atlama yok").color(GRAY));
            } else {
                ui.label(RichText::new(format!("! {} eksik numara:", s.missing_numbers.len())).color(WHITE));
                let nums: Vec<String> = s.missing_numbers.iter().take(500).map(|n| format!("{n:04}")).collect();
                ui.add(egui::Label::new(RichText::new(nums.join(", ")).color(GRAY).small()).wrap());
            }
        } else {
            ui.label(RichText::new("Kutuphane okunamadi.").color(WHITE));
        }

    }

    fn draw_settings(&mut self, ui: &mut Ui) {
        egui::ScrollArea::vertical().id_salt("settings").auto_shrink([false, false]).show(ui, |ui| {
            ui.label(RichText::new(format!("{STAR_MID} ayarlar")).font(bold(14.0)).color(WHITE));
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("muzik klasoru").color(DARK));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if tbtn(ui, "[o] klasoru ac").clicked() {
                        self.app.open_music_dir();
                    }
                    if tbtn(ui, "uygula").clicked() {
                        self.app.change_music_dir();
                    }
                    ui.add_sized(
                        vec2(ui.available_width().max(80.0), ui.spacing().interact_size.y),
                        egui::TextEdit::singleline(&mut self.app.dir_input),
                    );
                });
            });
            ui.horizontal(|ui| {
                ui.label(RichText::new("yt-dlp").color(DARK));
                ui.label(RichText::new(self.app.ytdlp_version.clone().unwrap_or_else(|| "...".into())).color(GRAY));
                if ui
                    .add_enabled(!self.app.is_downloading && !self.app.is_updating, egui::Button::new(RichText::new("[u] guncelle").color(GRAY)))
                    .clicked()
                {
                    self.app.start_ytdlp_update();
                    self.go(Screen::Downloader);
                }
            });
            ui.label(RichText::new("ayarlar ~/.config/muzikit/gui.json dosyasinda saklanir (ses, karistir, dongu, favoriler, klasor)").color(DARK).small());


            ui.add_space(14.0);
            ui.label(RichText::new(format!("{STAR_MID} gorunum")).font(bold(14.0)).color(WHITE));
            ui.add_space(4.0);
            let mut changed = false;
            ui.horizontal(|ui| {
                ui.label(RichText::new("tema").color(DARK));
                for th in Theme::ALL {
                    if tbtn_on(ui, th.label(), self.app.theme == th).clicked() && self.app.theme != th {
                        self.app.theme = th;
                        changed = true;
                    }
                }
            });
            ui.horizontal(|ui| {
                if toggle(ui, self.app.fx_enabled, "hareketli arka plan").clicked() {
                    self.app.fx_enabled = !self.app.fx_enabled;
                    changed = true;
                }
                if let Some(k) = self.key_hint(Action::ToggleFx) {
                    ui.label(RichText::new(format!("[{k}]")).color(DARK));
                }
            });
            ui.horizontal(|ui| {
                ui.label(RichText::new("panel saydamligi").color(DARK));
                let mut pct = 100.0 - self.app.panel_alpha as f32 / 255.0 * 100.0;
                if line_slider(ui, &mut pct, 0.0, 90.0, 5.0) {
                    self.app.panel_alpha = ((100.0 - pct) / 100.0 * 255.0).round() as u8;
                    changed = true;
                }
                ui.label(RichText::new(format!("%{:.0}", pct)).color(GRAY));
            });
            if changed {
                self.app.mark_dirty();
            }

            ui.add_space(14.0);
            self.draw_report_stats(ui);
        });
    }

    fn draw_shortcuts(&mut self, ui: &mut Ui) {
        let km_on = self.app.keymap.enabled;
        ui.horizontal(|ui| {
            if toggle(ui, km_on, "kisayollar etkin").clicked() {
                self.app.keymap.enabled = !km_on;
                self.app.mark_dirty();
            }
            ui.add_space(12.0);
            if tbtn(ui, "tumunu varsayilana dondur").clicked() {
                self.app.keymap.reset_all();
                self.capture = None;
                self.app.mark_dirty();
                self.app.notify("kisayollar varsayilana donduruldu");
            }
        });
        ui.label(
            RichText::new("tusa tikla > yeni tusa bas   ·   Esc: vazgec   ·   Del / sag tik: kaldir   ·   Ctrl/Alt/Shift kombinasyonlari olur")
                .color(DARK)
                .small(),
        );
        ui.add_space(6.0);

        let blink = ((self.start.elapsed().as_secs_f32() * 2.5) as u32).is_multiple_of(2);
        egui::ScrollArea::vertical().id_salt("shortcuts").auto_shrink([false, false]).show(ui, |ui| {
            let mut group = "";
            egui::Grid::new("shortcut_grid").num_columns(5).spacing(vec2(14.0, 4.0)).show(ui, |ui| {
                for a in Action::ALL {
                    if a.group() != group {
                        group = a.group();
                        ui.label(RichText::new(format!("// {group}")).font(bold(12.0)).color(GRAY));
                        ui.end_row();
                    }
                    let conf = self.app.keymap.get(a).clone();
                    let live = km_on && conf.enabled;

                    if toggle(ui, conf.enabled, "").on_hover_text("bu kisayolu ac / kapat").clicked() {
                        self.app.keymap.set_enabled(a, !conf.enabled);
                        self.app.mark_dirty();
                    }
                    ui.label(RichText::new(a.label()).color(if live { GRAY } else { DARK }));

                    for slot in 0..keymap::SLOTS {
                        let capturing = self.capture == Some((a, slot));
                        let (text, color) = if capturing {
                            (if blink { "tusa bas_" } else { "tusa bas " }.to_owned(), WHITE)
                        } else if let Some(b) = conf.keys.get(slot) {
                            (b.label(), if live { WHITE } else { DARK })
                        } else {
                            ("+".to_owned(), DIM.gamma_multiply(2.0))
                        };
                        let btn = egui::Button::new(RichText::new(text).font(mono(12.5)).color(color))
                            .min_size(vec2(96.0, 0.0))
                            .stroke(Stroke::new(1.0_f32, if capturing { WHITE } else { DIM }));
                        let resp = ui.add(btn).on_hover_text("tikla: yeni tus ata   ·   sag tik: kaldir");
                        if resp.clicked() {
                            self.capture = if capturing { None } else { Some((a, slot)) };
                        }
                        if resp.secondary_clicked() && slot < conf.keys.len() {
                            self.app.keymap.clear(a, slot);
                            self.app.mark_dirty();
                        }
                    }

                    if self.app.keymap.is_default(a) {
                        ui.label("");
                    } else if ui
                        .add(egui::Button::new(RichText::new("↺").color(GRAY)).frame(false))
                        .on_hover_text("varsayilana dondur")
                        .clicked()
                    {
                        self.app.keymap.reset(a);
                        self.app.mark_dirty();
                    }
                    ui.end_row();
                }
            });

            ui.add_space(10.0);
            ui.label(RichText::new("// sabit tuslar (degistirilemez)").font(bold(12.0)).color(GRAY));
            egui::Grid::new("fixed_keys").num_columns(2).spacing(vec2(18.0, 3.0)).show(ui, |ui| {
                for (k, v) in [
                    ("↑ ↓  PgUp PgDn  Home End", "listede / menude gez"),
                    ("Enter  /  cift tik", "cal / onayla"),
                    ("Esc", "geri / menu / kapat"),
                ] {
                    ui.label(RichText::new(k).color(GRAY));
                    ui.label(RichText::new(v).color(DARK));
                    ui.end_row();
                }
            });
        });
    }

    /// Ekrana gelebilecek tüm dış kaynaklı metinlerin karakterleri için yazı tipi olduğundan
    /// emin olur (şarkı adları, arama, link, loglar, bildirimler). Görülen karakterler
    /// bir kez denetlenir; bu yüzden her karede çağırmak ucuzdur.
    fn ensure_glyphs(&mut self, ctx: &egui::Context) {
        let app = &self.app;
        if self.seen_library_gen != app.library_gen {
            self.seen_library_gen = app.library_gen;
            let titles: String = app.songs.iter().map(|s| s.title.as_str()).collect();
            self.fonts.ensure(ctx, &titles);
        }
        if self.seen_log_gen != app.log_gen {
            let new = (app.log_gen - self.seen_log_gen).min(app.logs.len() as u64) as usize;
            self.seen_log_gen = app.log_gen;
            for line in &app.logs[app.logs.len() - new..] {
                self.fonts.ensure(ctx, line);
            }
        }
        for s in [&app.search_query, &app.url_input, &app.dir_input, &app.conv_input, &app.conv_output] {
            self.fonts.ensure(ctx, s);
        }
        if let Some(t) = &app.toast {
            self.fonts.ensure(ctx, &t.text);
        }
        if let Some(n) = &app.snap.track_name {
            self.fonts.ensure(ctx, n);
        }
    }

    fn update_title(&mut self, ctx: &egui::Context) {
        let title = match (self.app.snap.status, self.app.playing.is_some()) {
            (PlaybackStatus::Playing, true) => format!("▶ {} — muzikit", self.now_playing_name()),
            (PlaybackStatus::Paused, true) => format!("⏸ {} — muzikit", self.now_playing_name()),
            _ => "muzikit".to_string(),
        };
        if title != self.last_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.last_title = title;
        }
    }
}

impl eframe::App for Gui {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.app.tick();
        FX_ON.store(self.app.fx_enabled, Ordering::Relaxed);
        PANEL_ALPHA.store(self.app.panel_alpha, Ordering::Relaxed);
        self.ensure_glyphs(ctx);
        self.handle_keys(ctx);

        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(BLACK))
            .show(ctx, |ui| self.draw(ui));

        self.update_title(ctx);

        if self.app.should_quit {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if std::mem::take(&mut self.app.raise_requested) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }

        // Arka plan animasyonu: odaktayken akıcı (~25 fps); pencere arkadayken
        // yavaşlat, küçültülmüşken neredeyse durdur (CPU/GPU tasarrufu).
        let (focused, minimized) =
            ctx.input(|i| (i.viewport().focused.unwrap_or(true), i.viewport().minimized.unwrap_or(false)));
        let ms = if minimized {
            1000
        } else if !focused {
            200
        } else if self.app.fx_enabled || self.capture.is_some() {
            40
        } else {
            // Efektler kapalıyken sadece ilerleme çubuğu/saat için yeterli
            100
        };
        ctx.request_repaint_after(Duration::from_millis(ms));
    }
}
