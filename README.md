<div align="center">

# 🎵 muzikit

**YouTube'dan Opus indir, düzenli bir kütüphane tut, çal — Linux masaüstünde ve Android'de.<br>
İki cihaz aynı Wi-Fi'da kütüphanelerini şifreli olarak eşitler.**

![Linux](https://img.shields.io/badge/Linux-0.3.1-111111?style=for-the-badge&logo=linux&logoColor=white)
![Android](https://img.shields.io/badge/Android-1.2.1-111111?style=for-the-badge&logo=android&logoColor=white)

![Rust](https://img.shields.io/badge/Rust-egui-555555?style=flat-square&logo=rust&logoColor=white)
![Kotlin](https://img.shields.io/badge/Kotlin-Compose%20%2B%20Media3-555555?style=flat-square&logo=kotlin&logoColor=white)
![PipeWire](https://img.shields.io/badge/ses-PipeWire-555555?style=flat-square)
![Opus](https://img.shields.io/badge/biçim-Opus-555555?style=flat-square)
![Eşitleme](https://img.shields.io/badge/eşitleme-AES--256--GCM-555555?style=flat-square&logo=letsencrypt&logoColor=white)
![sudo](https://img.shields.io/badge/sudo-gerekmez-555555?style=flat-square)

[Özellikler](#-bir-bakışta) · [Linux](#-linux) · [Android](#-android) · [Eşitleme](#-eşitleme) · [Güvenlik](#-güvenlik) · [İndir](../../releases)

</div>

---

## ✦ Bir bakışta

| | 🐧 Linux | 📱 Android |
|---|:---:|:---:|
| YouTube / YouTube Music → Opus indirme, otomatik numara | ✅ | ✅ |
| `downloaded.txt` ile tekrar indirmeme | ✅ | ✅ |
| Yerleşik çalar, kuyruk, karıştırma, döngü, favoriler | ✅ | ✅ |
| Gerçek frekans analizörü (FFT) | ✅ | ✅ |
| Tema seçimi | ✅ siber / balık göleti | — |
| Uyku zamanlayıcısı | ✅ | ✅ |
| Klavye kısayolları | ✅ değiştirilebilir | ✅ sabit tuşlar (harici klavye) |
| Medya tuşları / kilit ekranı kontrolü | ✅ MPRIS | ✅ bildirim + kilit ekranı |
| MP3 → Opus toplu dönüştürücü | ✅ | — |
| Japonca/Korece vb. adlar için otomatik yazı tipi | ✅ | ✅ |
| Sadece kulaklık modu, kulaklık çıkınca durma | — | ✅ |
| Ana ekran widget'ı | — | ✅ |
| **Telefon ↔ bilgisayar şifreli eşitleme** | ✅ | ✅ |

Kütüphane düzeni iki tarafta aynıdır: `0817 - Şarkı adı.opus`. Numaralar asla değişmez; yeni şarkı bir sonraki numarayı alır.

## 📁 Depo yapısı

```
muzikit/
├── linux/      Masaüstü sürümü (Rust) — kaynak kodu
└── android/    Android sürümü (Kotlin) — kaynak kodu
```

Hazır programlar (`muzikit-linux-x86_64`, `muzikit-android-arm64.apk`) [Releases](../../releases) sayfasında.

---

## 🐧 Linux

Rust ile yazılmış, kendi penceresinde açılan (egui) masaüstü uygulaması. Siyah zemin, ince beyaz çerçeveler, sakin ve hareketli bir arka plan.

### Özellikler

**İndirici**
- YouTube ve YouTube Music linkleri; birden fazla link aynı anda (boşlukla ayır), sırayla indirilir.
- Playlist linkinde "tamamı mı, sadece bu şarkı mı?" sorusu.
- Dosya `NNNN - Başlık.opus` olarak sıradaki numarayla kaydedilir; var olan dosyanın üzerine asla yazılmaz.
- `downloaded.txt` ile daha önce indirilenler atlanır; yt-dlp program içinden güncellenir (`sudo` gerekmez).

**Çalar**
- PipeWire'a doğrudan bağlanan yerleşik ses motoru (Symphonia: Opus, MP3, FLAC, WAV, OGG, MKV).
- Sürüklenebilir ilerleme ve ses çubuğu, kuyruk (sağ tık → sıraya ekle / sıradaki), favoriler ★, son çalınanlar.
- Tekrarsız karıştırma, döngü (kapalı / liste / tek), uyku zamanlayıcısı (15–90 dk ya da şarkı sonu).
- Ses aygıtı değişince ya da akış koparsa kaldığı yerden otomatik devam.
- Medya tuşları ve KDE/GNOME medya denetimi (MPRIS), pencere küçükken de.

**Kütüphane ve araçlar**
- Çok kelimeli arama, numara ↑↓ / isim sıralaması, "çalanı göster".
- Kütüphane sağlık raporu: eksik numaralar, `downloaded.txt` kaydı.
- MP3 → Opus toplu dönüştürücü (bitrate seçimi).
- Yeniden atanabilir kısayollar (eylem başına 2 tuş, Ctrl/Alt/Shift), `?` ile kısayol yardımı.
- İki tema: **siber** (ızgara, veri akışı, radar, frekans analizörü — FFT, 24 bant, 30 Hz–16 kHz) ya da **balık göleti** (yukarıdan bakılan, gölgelendirilmiş ASCII balıklar; birbirine çarpmadan yüzer). Ayarlar → görünüm.
- Ayarlanabilir panel saydamlığı; hareketli arka plan tek tuşla (`v`) kapatılabilir.

### Kurulum

[Releases](../../releases) sayfasından `muzikit-linux-x86_64` dosyasını indir, sonra:

```bash
install -m 755 muzikit-linux-x86_64 ~/.local/bin/muzikit
muzikit
```

Gereken: 64 bit Linux, PipeWire (`pipewire-pulse`), `PATH` içinde [`yt-dlp`](https://github.com/yt-dlp/yt-dlp) ve `ffmpeg`.

### Komut satırı

```bash
muzikit                                # arayüz
muzikit download "https://youtu.be/…"  # doğrudan indir
muzikit convert ~/Müzik/mp3            # MP3 → Opus
muzikit check                          # kütüphane raporu
muzikit update                         # yt-dlp güncelle
```

---

<!-- ANDROID -->
## 📱 Android

Telefon için Kotlin, Jetpack Compose ve Media3 ile yazılmış yerel uygulama. Masaüstündeki kütüphane düzenini (`0817 - Şarkı adı.opus`) birebir kullanır; aynı klasör iki tarafta da aynı görünür.

### Özellikler

**Çalma**
- Ekran kapalıyken ve uygulama arka plandayken çalmaya devam eder; arka planda hiçbir animasyon çalışmaz.
- Bildirim ve kilit ekranından önceki/sonraki, ileri-geri sarma ve ses düzeyi.
- Ses sıfıra indirilince duraklar, ses geri gelince kaldığı yerden devam eder (isteğe bağlı).
- Telefon çaldığında ya da başka uygulama ses çaldığında durur.
- Bluetooth kulaklık bağlanınca devam eder (isteğe bağlı).
- Karıştırma, döngü (kapalı / liste / tek şarkı), kuyruk ve "sıradaki yap".
- Uyku zamanlayıcısı: 15–90 dakika ya da "şarkı bitince".
- Uygulama kapanıp açılınca son şarkı ve konum geri gelir.

**Sadece kulaklık modu**
- Açıkken ses yalnızca kablolu, USB ya da Bluetooth kulaklığa gider. Kulaklık çıkarılırsa müzik anında durur.
- Çalmadan önce sesin gerçekten kulaklığa yönlendiği doğrulanır.
- Telefon hoparlörünün medya sesi 0'a çekilir; hoparlörden ses sızmaz.

**Kütüphane**
- Numaraya göre ↑↓ ya da isme göre sıralama, çok kelimeli arama, favoriler.
- Kütüphane sağlık raporu: numarasız dosyalar, aynı numaralı dosyalar, eksik numaralar.
- Harici klavye kısayolları (masaüstüyle aynı tuşlar).
- Ana ekran widget'ı.

**Görünüm**
- Gerçek frekans analizörü (FFT, 30 Hz–16 kHz) ve sakin, hareketli bir arka plan; ikisi de kapatılabilir.
- Akıcı kaydırma için çizimler tek zamanlayıcıdan beslenir; görünmeyen ekranda çizim yapılmaz.

**İndirici**
- YouTube / YouTube Music linkinden Opus olarak indirir; birden fazla link boşlukla ayrılabilir.
- Playlist linkinde "tamamı mı, sadece bu şarkı mı?" diye sorar.
- Kapak resmi gömülmez, açıklama ve diğer etiketler silinir; dosya sıradaki numarayla adlandırılır.
- Daha önce indirilenler `downloaded.txt` ile atlanır. Yaş sınırlı videolar atlanır.
- Ekran kapalıyken de indirmeye devam eder. yt-dlp uygulama içinden güncellenebilir.

**Bilgisayarla eşitleme**
- Aynı Wi-Fi'daki muzikit masaüstüyle iki yönlü eşitleme. Ayrıntılar aşağıdaki "Eşitleme" bölümünde.
- "Eşitleme geçmişini sil" ile tüm eşleşmeler unutulur; müzik dosyalarına dokunulmaz.

### Kurulum

1. [Releases](../../releases) sayfasından `muzikit-android-arm64.apk` dosyasını indir.
2. Telefonda dosyayı aç → **Yükle** (bilinmeyen kaynaklardan yükleme izni istenebilir).
3. İlk açılışta müzik klasörünü seç.

Gereken: Android 10 veya üstü, 64 bit ARM (arm64-v8a) telefon.

### İzinler

| İzin | Neden |
|---|---|
| Ön plan servisi (medya, veri eşitleme) | Arka planda çalma ve indirme |
| Uyanık tutma | Ekran kapalıyken çalmanın kesilmemesi |
| İnternet, ağ durumu | İndirici ve yerel ağda eşitleme |
| Bildirim | Çalma ve indirme bildirimi |

Mikrofon, konum ya da depolama izni **istenmez**. Müzik klasörüne yalnızca senin seçtiğin klasör için Android'in klasör seçicisiyle (SAF) erişilir. Uygulamanın verileri yedeklenmez (`allowBackup=false`).

### Kaynaktan derleme

```bash
cd android
./derle.sh
```

JDK 17+ ve Android SDK gerekir (`ANDROID_HOME`, varsayılan `~/Android/Sdk`). Testler çalışır ve imzalı APK `../.build/android/app/outputs/apk/release/` altına yazılır; `android/` klasörünün içine derleme artığı düşmez. Kendi imza anahtarın yoksa APK hata ayıklama anahtarıyla imzalanır.
<!-- /ANDROID -->

---

## 🔄 Eşitleme

Aynı Wi-Fi'daki telefon ile bilgisayarın kütüphanelerini tek tuşla eşitler.

1. Telefonda **Ayarlar → bilgisayarla eşitle → eşitlemeyi aç**.
2. Bilgisayarda **Ayarlar & Kısayollar → esitle → telefonu bul** (bulunamazsa telefonda yazan IP'yi gir).
3. İlk seferde telefondaki 12 karakterlik kodu bilgisayara bir kez gir.
4. Bilgisayar planı gösterir — *kaç şarkı gelecek, kaç şarkı gidecek, hangi numara kayacak* — sen onaylayınca uygular.

**Kurallar**
- Şarkının kimliği dosyanın SHA-256'sıdır; aynı şarkı iki kez taşınmaz.
- **Var olan numaralar değişmez.** Yalnız bir tarafta olan şarkı karşıya aynı numarayla gider.
- İki cihaz birbirinden habersiz aynı yeni numarayı farklı şarkılara verdiyse önce eklenen numarasını korur, diğeri sona kayar (favorisi de taşınır).
- `downloaded.txt` iki tarafta aynı olur. Silinen şarkı karşı tarafta silinmez.
- **Eşitleme geçmişini sil** tuşu (iki tarafta) eşleşmeleri sıfırlar; müziklere dokunmaz.

İki uygulama aynı algoritmayı birebir uygular ve ortak test örnekleriyle doğrulandı.

---

## 🔒 Güvenlik

**Eşitleme**
- Sadece yerel ağ (10/8, 172.16/12, 192.168/16); telefon tarafı yalnız eşitleme ekranı açıkken ve en fazla 10 dakika dinler. Bilgisayar sunucu açmaz; telefonu yalnız kısa bir mDNS aramasıyla bulur.
- Her istek ve yanıt **HMAC-SHA256** ile imzalı (±60 sn zaman, tek kullanımlık nonce); gövdeler **AES-256-GCM** ile şifreli, parça sırası ve kesme korumalı.
- Anahtar, 12 karakterlik eşleştirme kodundan HKDF ile türetilir; kod 3 hatalı denemede yenilenir. Anahtar bilgisayarda `~/.config/muzikit/sync-key` (izin 0600), telefonda uygulamanın özel alanında.
- Gelen dosyanın SHA-256'sı tutmazsa atılır; dosya adı deseni, `..`, `/` ve görünmez yön karakterleri reddedilir; 100 MB sınırı; hiçbir dosyanın üzerine yazılmaz.

**İndirici ve dönüştürücü**
- Yalnız izinli YouTube alan adları kabul edilir; linkler sabit biçime çevrilir ve yt-dlp'ye `--` ayırıcısıyla, `--ignore-config` ile verilir.
- Dosya adlarından yol, kontrol ve yön karakterleri temizlenir; terminal kaçış dizileri loglara geçmez.
- ffmpeg girdisi yalnız yerel dosya ve yalnız MP3 olarak açılır (`-protocol_whitelist file -f mp3`).

**Android**
- Dışa açık bileşen yalnız başlatıcı, widget ve medya servisi; `allowBackup=false`; mikrofon, konum, depolama izni yok (klasör erişimi SAF).

Bağımlılık taraması: `cargo audit` — bilinen açık yok. Rust kodunda `unsafe` yok.

---

## 🛠️ Kaynaktan derleme

```bash
# Linux
cd linux && cargo test && cargo build --release      # → target/release/muzikit

# Android (JDK 17+, Android SDK)
cd android && ./derle.sh                              # → ../.build/android/app/outputs/apk/release/
```

---

## ⚠️ Not

YouTube'dan indirmek YouTube kullanım şartlarına aykırı olabilir; muzikit kişisel kullanım içindir. Android sürümündeki
[youtubedl-android](https://github.com/JunkFood02/youtubedl-android) (yt-dlp, Python, FFmpeg) GPL-3.0 lisanslıdır.
