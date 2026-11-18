//! "Balık göleti" teması: yukarıdan bakılan, gölgelendirilmiş ASCII balıklar.
//! Balıklar rastgele dolaşır, komşularına çarpmamak için yön değiştirir; yazıların altında soluk çizilir.

use eframe::egui::{self, Align2, Color32, FontFamily, FontId, Painter, Pos2, Rect, vec2};

fn mono(size: f32) -> FontId {
    FontId::new(size, FontFamily::Monospace)
}

fn star_hash(x: u32, y: u32) -> u32 {
    let h = x.wrapping_mul(374_761_393) ^ y.wrapping_mul(668_265_263) ^ x.wrapping_add(y).wrapping_mul(1_442_695_041);
    (h ^ (h >> 13)).wrapping_mul(1_274_126_177)
}

// ── Gölet: yukarıdan bakılan, 3B gölgelendirilmiş ASCII balıklar ────────
// Her balık bir omurga (izlediği yol) boyunca dizilmiş karakterlerden oluşur.
// Gövde enine kesiti elips kabul edilip ışığa göre gölgelendirilir; parlak
// sırt ve karanlık kenarlar karakter rampasıyla (" .:-=+*#%@") hacim hissi verir.
// Derindeki balıklar daha küçük ve soluk, yüzeye yakın olanlar büyük ve parlaktır.

/// Panel/yazı altında kalan balıkların parlaklık çarpanı
const DIM: f32 = 0.28;

const SHADE_RAMP: [&str; 9] = [".", ":", "-", "=", "+", "*", "#", "%", "@"];
// Baştan kuyruğa gövde yarı genişliği (piksel, ölçek 1 için)
const BODY_PROFILE: [f32; 10] = [3.5, 6.0, 7.5, 8.0, 7.5, 6.5, 5.0, 3.6, 2.4, 1.4];
const SEG_SPACING: f32 = 6.0;
const TRAIL_STEP: f32 = 1.5;

struct Koi {
    pos: egui::Vec2, // gölet sol üst köşesine göre
    heading: f32,
    base_speed: f32,
    z: f32, // 0 = dipte, 1 = yüzeyde
    seed: u32,
    phase: f32,
    trail: std::collections::VecDeque<egui::Vec2>,
}

impl Koi {
    fn scale(&self) -> f32 {
        0.75 + 0.6 * self.z
    }
}

#[derive(Default)]
pub struct Pond {
    fish: Vec<Koi>,
    last_t: f32,
}

fn rand01(seed: u32, k: u32) -> f32 {
    (star_hash(seed, k) % 10_000) as f32 / 10_000.0
}

fn wrap_angle(a: f32) -> f32 {
    let tau = std::f32::consts::TAU;
    (a + std::f32::consts::PI).rem_euclid(tau) - std::f32::consts::PI
}

/// Karakteri `c` merkezli ve `angle` kadar döndürülmüş çizer.
fn glyph(p: &Painter, c: Pos2, s: &str, font: &FontId, col: Color32, angle: f32) {
    let galley = p.layout_no_wrap(s.to_owned(), font.clone(), col);
    let pos = c - galley.rect.center().to_vec2();
    p.add(egui::epaint::TextShape::new(pos, galley, col).with_angle_and_anchor(angle, Align2::CENTER_CENTER));
}

fn gray(v: f32) -> Color32 {
    let v = v.clamp(0.0, 255.0) as u8;
    Color32::from_rgb(v, v, v)
}

impl Pond {
    fn spawn(&mut self, size: egui::Vec2) {
        let count = ((size.x * size.y) / 30_000.0).clamp(12.0, 40.0) as u32;
        self.fish = (0..count)
            .map(|i| {
                let seed = star_hash(i, 4242);
                Koi {
                    pos: vec2(rand01(seed, 1) * size.x, rand01(seed, 2) * size.y),
                    heading: rand01(seed, 3) * std::f32::consts::TAU,
                    base_speed: 22.0 + rand01(seed, 4) * 26.0,
                    z: rand01(seed, 5),
                    seed,
                    phase: rand01(seed, 6) * 10.0,
                    trail: Default::default(),
                }
            })
            .collect();
        // Derindekiler önce çizilsin
        self.fish.sort_by(|a, b| a.z.total_cmp(&b.z));
    }

    fn update(&mut self, size: egui::Vec2, t: f32) {
        if self.fish.is_empty() {
            self.spawn(size);
        }
        let dt = (t - self.last_t).clamp(0.0, 0.1);
        self.last_t = t;
        let center = size * 0.5;
        let margin = 70.0;

        // Çarpışmadan kaçınma: her balık önündeki/yanındaki komşuları
        // (baş ve gövde ortası) algılar ve ondan uzaklaşan yöne döner.
        let bodies: Vec<(egui::Vec2, egui::Vec2, f32)> = self
            .fish
            .iter()
            .map(|f| {
                let mid = f.trail.get(f.trail.len() / 2).copied().unwrap_or(f.pos);
                (f.pos, mid, f.scale())
            })
            .collect();
        let avoid: Vec<(f32, egui::Vec2)> = self
            .fish
            .iter()
            .enumerate()
            .map(|(i, f)| {
                let dir = vec2(f.heading.cos(), f.heading.sin());
                let mut turn = 0.0_f32;
                let mut push = egui::Vec2::ZERO;
                for (j, &(head, mid, sj)) in bodies.iter().enumerate() {
                    if i == j {
                        continue;
                    }
                    let radius = 46.0 * (f.scale() + sj);
                    let min_gap = 22.0 * (f.scale() + sj);
                    for (k, q) in [head, mid].into_iter().enumerate() {
                        let d = q - f.pos;
                        let dist = d.length();
                        if dist < 1e-3 || dist > radius {
                            continue;
                        }
                        // Çok yakınsa ne yöne bakarsa baksın birbirinden it
                        if dist < min_gap {
                            push -= d / dist * (min_gap - dist);
                        }
                        let ahead = dir.dot(d) / dist; // 1 = tam önde, -1 = arkada
                        if ahead < -0.3 {
                            continue;
                        }
                        // Komşu sağdaysa sola, soldaysa sağa dön
                        let cross = dir.x * d.y - dir.y * d.x;
                        let side = if cross > 0.0 { -1.0 } else { 1.0 };
                        let close = 1.0 - dist / radius;
                        let w = if k == 0 { 1.0 } else { 0.8 };
                        turn += side * close * close * (0.6 + ahead) * w;
                    }
                }
                (turn, push)
            })
            .collect();

        for (f, (avoid, push)) in self.fish.iter_mut().zip(avoid) {
            // Rastgele ama yumuşak dolaşma
            let wander = 0.6 * (t * 0.37 + f.phase).sin() + 0.45 * (t * 0.83 + f.phase * 2.3).sin();
            f.heading += wander * 0.9 * dt;
            f.heading += avoid.clamp(-3.0, 3.0) * 4.0 * dt;
            f.pos += push * (2.5 * dt).min(0.5);

            // Kenara yaklaşınca göletin ortasına dön
            let edge = [
                margin - f.pos.x,
                f.pos.x - (size.x - margin),
                margin - f.pos.y,
                f.pos.y - (size.y - margin),
            ]
            .into_iter()
            .fold(0.0_f32, f32::max);
            if edge > 0.0 {
                let to_c = center - f.pos;
                let want = to_c.y.atan2(to_c.x);
                let diff = wrap_angle(want - f.heading);
                f.heading += diff * (edge / margin).min(1.5) * 2.2 * dt;
            }

            let speed = f.base_speed * (1.0 + 0.3 * (t * 0.5 + f.phase).sin());
            let dir = vec2(f.heading.cos(), f.heading.sin());
            f.pos += dir * speed * dt;
            f.pos.x = f.pos.x.clamp(-60.0, size.x + 60.0);
            f.pos.y = f.pos.y.clamp(-60.0, size.y + 60.0);

            let need = (BODY_PROFILE.len() as f32 * SEG_SPACING * f.scale() / TRAIL_STEP) as usize + 8;
            if f.trail.front().is_none_or(|&last| (f.pos - last).length() >= TRAIL_STEP) {
                f.trail.push_front(f.pos);
            }
            f.trail.truncate(need);
        }
    }

    /// İzlenen yoldan eşit aralıklı omurga noktaları çıkarır (baştan kuyruğa).
    fn spine(f: &Koi, n: usize, spacing: f32) -> Vec<egui::Vec2> {
        let mut pts = Vec::with_capacity(n);
        pts.push(f.pos);
        let mut prev = f.pos;
        let mut acc = 0.0;
        let mut want = spacing;
        for &q in f.trail.iter() {
            let seg = (q - prev).length();
            while seg > 0.0 && acc + seg >= want && pts.len() < n {
                let k = (want - acc) / seg;
                pts.push(prev + (q - prev) * k);
                want += spacing;
            }
            acc += seg;
            prev = q;
            if pts.len() >= n {
                break;
            }
        }
        // Yol henüz kısaysa düz geriye doğru uzat
        let back = -vec2(f.heading.cos(), f.heading.sin());
        while pts.len() < n {
            let last = *pts.last().unwrap();
            pts.push(last + back * spacing);
        }
        pts
    }

    /// `dim`: bu alanların altında kalan balıklar soluk çizilir (üstteki yazılar okunsun diye).
    pub fn draw(&mut self, p: &Painter, area: Rect, t: f32, dim: &[Rect]) {
        self.update(area.size(), t);
        draw_ripples(p, area, t, dim);
        let glyph = |p: &Painter, c: Pos2, s: &str, font: &FontId, col: Color32, angle: f32| {
            let col = if dim.iter().any(|r| r.contains(c)) { col.gamma_multiply(DIM) } else { col };
            glyph(p, c, s, font, col, angle)
        };

        // Işık sol üstten, yukarıdan gelir (x sağ, y aşağı, z ekrandan dışarı)
        let light = {
            let l = [-0.45_f32, -0.55, 0.70];
            let n = (l[0] * l[0] + l[1] * l[1] + l[2] * l[2]).sqrt();
            [l[0] / n, l[1] / n, l[2] / n]
        };

        for f in &self.fish {
            let s = f.scale();
            let depth = 0.45 + 0.55 * f.z; // derindekiler soluk
            let font = mono(8.5 + 4.0 * f.z);
            let n = BODY_PROFILE.len();
            let mut spine = Self::spine(f, n, SEG_SPACING * s);

            // Yüzme dalgası: kuyruğa doğru artan yanal salınım
            let swim = t * (6.0 + f.base_speed * 0.08) + f.phase;
            let tangents: Vec<egui::Vec2> = (0..n)
                .map(|k| {
                    let a = spine[k.saturating_sub(1)];
                    let b = spine[(k + 1).min(n - 1)];
                    let d = a - b;
                    if d.length() > 1e-3 { d.normalized() } else { vec2(f.heading.cos(), f.heading.sin()) }
                })
                .collect();
            for k in 0..n {
                let perp = vec2(-tangents[k].y, tangents[k].x);
                let amp = 2.2 * s * (k as f32 / n as f32).powf(1.5);
                spine[k] += perp * amp * (swim - k as f32 * 0.85).sin();
            }

            let to_pos = |v: egui::Vec2| area.left_top() + v;

            // Göğüs yüzgeçleri (çırpınarak)
            let flap = (swim * 0.7).sin();
            for (k, reach, len) in [(2usize, 1.0_f32, 3), (6, 0.7, 2)] {
                let dir = tangents[k];
                let perp = vec2(-dir.y, dir.x);
                let ang = dir.y.atan2(dir.x);
                let w = BODY_PROFILE[k] * s;
                for side in [-1.0_f32, 1.0] {
                    let ch = if side < 0.0 { "\\" } else { "/" };
                    for j in 1..=len {
                        let jf = j as f32;
                        let c = spine[k] + perp * side * (w + jf * 3.6 * s * reach)
                            - dir * (jf * 2.6 * s + 1.5 * s * (1.0 + side * flap));
                        glyph(p, to_pos(c), ch, &font, gray(150.0 * depth - jf * 18.0), ang);
                    }
                }
            }

            // Kuyruk yüzgeci (iki lob)
            {
                let k = n - 1;
                let dir = tangents[k];
                let perp = vec2(-dir.y, dir.x);
                let ang = dir.y.atan2(dir.x);
                let sway = (swim - n as f32 * 0.85).sin() * 3.0 * s;
                for side in [-1.0_f32, 1.0] {
                    let ch = if side < 0.0 { "\\" } else { "/" };
                    for j in 1..=3 {
                        let jf = j as f32;
                        let c = spine[k] - dir * jf * 3.8 * s + perp * (side * jf * 2.8 * s + sway * jf / 3.0);
                        glyph(p, to_pos(c), ch, &font, gray(185.0 * depth - jf * 20.0), ang);
                    }
                }
            }

            // Gövde: her omurga noktasında enine örnekler, elips normali ile gölgelendirme
            let step = 5.2 * s;
            for k in 0..n {
                let dir = tangents[k];
                let perp = vec2(-dir.y, dir.x);
                let ang = dir.y.atan2(dir.x);
                let w = BODY_PROFILE[k] * s;
                let m = ((2.0 * w / step).round() as usize).max(1);
                // Baş ve kuyruk uçları öne/arkaya doğru eğimli: biraz daha karanlık
                let end_fall = 1.0 - 0.25 * ((k as f32 / (n - 1) as f32) * 2.0 - 1.0).abs().powi(3);
                for i in 0..m {
                    let u = if m == 1 { 0.0 } else { -1.0 + (2.0 * i as f32 + 1.0) / m as f32 };
                    let nz = (1.0 - u * u).max(0.0).sqrt();
                    let nrm = [perp.x * u, perp.y * u, nz];
                    let lambert = (nrm[0] * light[0] + nrm[1] * light[1] + nrm[2] * light[2]).max(0.0);
                    let mut b = (0.12 + 0.88 * lambert) * end_fall;
                    // Koi benekleri: balığa özgü desen
                    let spot = star_hash(f.seed, (k as u32) * 7 + ((u + 1.0) * 1.5) as u32).is_multiple_of(5);
                    if spot {
                        b *= 0.5;
                    }
                    let c = to_pos(spine[k] + perp * u * w);
                    // Gözler: başın iki yanında
                    if k == 1 && m >= 2 && u.abs() > 0.3 {
                        glyph(p, c, "o", &font, gray(255.0 * depth), ang);
                        continue;
                    }
                    let idx = ((b * 8.99) as usize).min(SHADE_RAMP.len() - 1);
                    glyph(p, c, SHADE_RAMP[idx], &font, gray((45.0 + 210.0 * b) * depth), ang);
                }
            }
        }
    }
}

/// Su yüzeyinde genişleyip sönen halkalar.
fn draw_ripples(p: &Painter, area: Rect, t: f32, dim: &[Rect]) {
    let font = mono(10.0);
    let count = ((area.width() * area.height()) / 160_000.0).clamp(2.0, 6.0) as u32;
    for i in 0..count {
        let period = 4.0 + rand01(i, 77) * 3.0;
        let tt = t + rand01(i, 78) * period;
        let cycle = (tt / period).floor() as u32;
        let ph = (tt % period) / period;
        let c = area.left_top()
            + vec2(rand01(i * 31 + cycle, 79) * area.width(), rand01(i * 31 + cycle, 80) * area.height());
        for ring in 0..2 {
            let ph = ph - ring as f32 * 0.18;
            if !(0.0..1.0).contains(&ph) {
                continue;
            }
            let r = 6.0 + ph * 70.0;
            let pts = ((std::f32::consts::TAU * r) / 10.0).max(6.0) as u32;
            let col = gray(90.0 * (1.0 - ph).powi(2));
            for j in 0..pts {
                let a = j as f32 / pts as f32 * std::f32::consts::TAU;
                // Perspektif: halkalar hafif basık
                let q = c + vec2(a.cos() * r, a.sin() * r * 0.85);
                let col = if dim.iter().any(|r| r.contains(q)) { col.gamma_multiply(DIM) } else { col };
                p.text(q, Align2::CENTER_CENTER, ".", font.clone(), col);
            }
        }
    }
}
