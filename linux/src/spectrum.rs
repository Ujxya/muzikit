//! Ana sayfadaki FREQ//ANALYZER için gerçek frekans analizi.
//!
//! Oynatıcının sunucuya gönderdiği ses örneklerinden (tampon gecikmesi kadar geriden,
//! yani o an duyulan kısım) Hann pencereli FFT alınır; enerji 24 logaritmik banda
//! toplanır ve desibel ölçeğine çevrilir. Çubuklar hızlı yükselir, yavaş düşer;
//! ölçek son zamanlardaki en yüksek seviyeye göre kendini ayarlar (otomatik kazanç).

use std::f32::consts::PI;

pub const BANDS: usize = 24;
const FFT_N: usize = 2048;
const F_LO: f32 = 30.0;
const F_HI: f32 = 16_000.0;
/// Görünen dinamik aralık (dB): en yüksek bandın bu kadar altı sıfır çubuk
const RANGE_DB: f32 = 48.0;

/// Yerinde, radix-2 FFT (uzunluk 2'nin kuvveti olmalı).
fn fft(re: &mut [f32], im: &mut [f32]) {
    let n = re.len();
    debug_assert!(n.is_power_of_two() && im.len() == n);
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let ang = -2.0 * PI / len as f32;
        let (wr, wi) = (ang.cos(), ang.sin());
        for start in (0..n).step_by(len) {
            let (mut cr, mut ci) = (1.0_f32, 0.0_f32);
            for k in 0..len / 2 {
                let a = start + k;
                let b = a + len / 2;
                let vr = re[b] * cr - im[b] * ci;
                let vi = re[b] * ci + im[b] * cr;
                re[b] = re[a] - vr;
                im[b] = im[a] - vi;
                re[a] += vr;
                im[a] += vi;
                let ncr = cr * wr - ci * wi;
                ci = cr * wi + ci * wr;
                cr = ncr;
            }
        }
        len <<= 1;
    }
}

/// Örneklerden bant başına dB değerleri (son `FFT_N` örnek kullanılır).
pub fn band_db(samples: &[f32], rate: u32) -> [f32; BANDS] {
    let mut out = [-120.0; BANDS];
    if samples.len() < FFT_N || rate == 0 {
        return out;
    }
    let s = &samples[samples.len() - FFT_N..];
    let mut re: Vec<f32> = s
        .iter()
        .enumerate()
        .map(|(i, x)| x * (0.5 - 0.5 * (2.0 * PI * i as f32 / (FFT_N - 1) as f32).cos()))
        .collect();
    let mut im = vec![0.0; FFT_N];
    fft(&mut re, &mut im);

    let bin_hz = rate as f32 / FFT_N as f32;
    let nyquist_bins = FFT_N / 2;
    for (b, o) in out.iter_mut().enumerate() {
        let lo = F_LO * (F_HI / F_LO).powf(b as f32 / BANDS as f32);
        let hi = F_LO * (F_HI / F_LO).powf((b + 1) as f32 / BANDS as f32);
        let k0 = ((lo / bin_hz).floor() as usize).clamp(1, nyquist_bins - 1);
        let k1 = ((hi / bin_hz).ceil() as usize).clamp(k0 + 1, nyquist_bins);
        let energy: f32 = (k0..k1).map(|k| re[k] * re[k] + im[k] * im[k]).sum::<f32>() / (k1 - k0) as f32;
        *o = 10.0 * (energy + 1e-12).log10();
    }
    out
}

/// Ekrandaki çubukların durumu (0..1).
pub struct Spectrum {
    pub levels: [f32; BANDS],
    peak_db: f32,
}

impl Default for Spectrum {
    fn default() -> Self {
        Self { levels: [0.0; BANDS], peak_db: -30.0 }
    }
}

impl Spectrum {
    /// `samples`: o an duyulan ses (yoksa sessizlik, çubuklar yere iner). `dt`: saniye.
    pub fn update(&mut self, samples: Option<(&[f32], u32)>, dt: f32) {
        let dt = dt.clamp(0.0, 0.25);
        let target = match samples {
            Some((s, rate)) => {
                let db = band_db(s, rate);
                let frame_peak = db.iter().copied().fold(f32::MIN, f32::max);
                // Otomatik kazanç: tepe hemen yükselir, yavaşça (6 dB/sn) iner
                self.peak_db = frame_peak.max(self.peak_db - 6.0 * dt).max(-60.0);
                db.map(|d| ((d - (self.peak_db - RANGE_DB)) / RANGE_DB).clamp(0.0, 1.0))
            }
            None => [0.0; BANDS],
        };
        for (l, t) in self.levels.iter_mut().zip(target) {
            if t > *l {
                // Hızlı yükseliş
                *l += (t - *l) * (1.0 - (-dt * 30.0).exp());
            } else {
                // Yavaş düşüş (saniyede ~1.6 çubuk boyu)
                *l = (*l - 1.6 * dt).max(t);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(freq: f32, rate: u32) -> Vec<f32> {
        (0..FFT_N).map(|i| (2.0 * PI * freq * i as f32 / rate as f32).sin() * 0.5).collect()
    }

    fn loudest(db: &[f32; BANDS]) -> usize {
        (0..BANDS).max_by(|&a, &b| db[a].total_cmp(&db[b])).unwrap()
    }

    fn band_of(freq: f32) -> usize {
        ((freq / F_LO).ln() / (F_HI / F_LO).ln() * BANDS as f32) as usize
    }

    #[test]
    fn tones_land_in_their_band() {
        for f in [60.0, 440.0, 1000.0, 5000.0, 12000.0] {
            let db = band_db(&tone(f, 48_000), 48_000);
            let got = loudest(&db) as i32;
            let want = band_of(f) as i32;
            assert!((got - want).abs() <= 1, "{f} Hz: bant {got}, beklenen {want}");
        }
    }

    #[test]
    fn silence_drops_bars_and_sound_raises_them() {
        let mut s = Spectrum::default();
        let t = tone(1000.0, 48_000);
        for _ in 0..10 {
            s.update(Some((&t, 48_000)), 0.04);
        }
        let b = band_of(1000.0);
        assert!(s.levels[b] > 0.8, "ton bandi yukselmeli: {}", s.levels[b]);
        assert!(s.levels[0] < 0.3, "uzak bant dusuk kalmali: {}", s.levels[0]);
        for _ in 0..40 {
            s.update(None, 0.04);
        }
        assert!(s.levels.iter().all(|&l| l == 0.0));
    }
}
