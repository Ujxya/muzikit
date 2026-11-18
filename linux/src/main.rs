mod app;
mod config;
mod converter;
mod downloader;
mod fontfallback;
mod gui;
mod keymap;
mod library;
mod mpris;
mod player;
mod pond;
mod settings;
mod spectrum;
mod sync;
mod utils;

use std::path::PathBuf;
use anyhow::Result;
use clap::{Parser, Subcommand};

use crate::config::Config;
use crate::app::App;

#[derive(Parser)]
#[command(name = "muzikit")]
#[command(version = "0.3.1")]
#[command(about = "Rust ile yazılmış masaüstü (GUI) müzik indirme, Opus dönüştürme ve müzik çalar", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Müziklerin saklandığı ana çıktı dizini (Belirtilmezse ~/Müzik/Müzik otomatik algılanır)
    #[arg(short, long)]
    dir: Option<PathBuf>,

    /// İndirilen video ID'lerinin tutulduğu dosya
    #[arg(short, long)]
    log: Option<PathBuf>,
}

#[derive(Subcommand)]
enum Commands {
    /// YouTube URL veya playlistinden müzik indirip Opus'a çevirir (CLI modu)
    Download {
        /// İndirilecek video veya playlist URL'si
        url: String,
    },
    /// Klasördeki MP3 dosyalarını topluca Opus formatına dönüştürür (CLI modu)
    Convert {
        /// MP3 dosyalarının bulunduğu klasör
        #[arg(default_value = "./")]
        input: PathBuf,
        /// Çıktı klasörü
        #[arg(short, long, default_value = "./opus_output")]
        output: PathBuf,
        /// Hedef bitrate (örn: 128k, 96k, 160k)
        #[arg(short, long, default_value = "128k")]
        bitrate: String,
    },
    /// Kütüphanedeki şarkı numaralarını ve downloaded.txt kütüğünü denetler
    Check,
    /// Kurulu yt-dlp aracını en son kararlı sürüme günceller (sudo gerekmez)
    Update,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Otomatik dizin tespiti: ~/Müzik/Müzik varsa onu seçer
    let mut config = Config::default_detected();
    let dir_given = cli.dir.is_some();
    if let Some(d) = cli.dir {
        config.music_dir = d;
    }
    if let Some(l) = cli.log {
        config.log_file = l;
    }

    match cli.command {
        Some(Commands::Download { url }) => {
            println!("🚀 İndirme başlatılıyor: {}", utils::strip_control_chars(&url));
            let Some((processed_url, is_pl)) = utils::normalize_url(&url) else {
                eprintln!("❌ Güvenlik: Sadece https://youtube.com / youtu.be / music.youtube.com linkleri kabul edilir.");
                std::process::exit(2);
            };
            let ids = if is_pl {
                downloader::extract_playlist_ids(&processed_url)?
            } else if let Some(vid) = utils::extract_video_id(&processed_url) {
                vec![vid]
            } else {
                eprintln!("Geçerli bir video ID'si bulunamadı.");
                return Ok(());
            };

            let next_num = library::get_next_number(&config.music_dir);
            downloader::process_download_queue(ids, &config.music_dir, &config.log_file, next_num, None)?;
        }
        Some(Commands::Convert { input, output, bitrate }) => {
            converter::batch_convert(&input, &output, &bitrate, None)?;
        }
        Some(Commands::Check) => {
            let stats = library::inspect_library(&config.music_dir, &config.log_file)?;
            println!("\n--- Kütüphane Sağlık ve Analiz Raporu ---");
            println!("📁 Taranan Dizin: {}", config.music_dir.display());
            println!("🎵 Toplam Opus Dosyası: {}", stats.total_opus_files);
            println!("🔢 En Yüksek Şarkı Numarası: {:04}", stats.max_number);
            println!("📝 downloaded.txt Kayıt Sayısı: {}", stats.downloaded_txt_count);
            if stats.missing_numbers.is_empty() {
                println!("✓ Şarkı numaralandırmasında hiçbir eksik/atlama yok. Harika!");
            } else {
                println!("⚠ Toplam {} adet eksik numara tespit edildi: {:?}", stats.missing_numbers.len(), stats.missing_numbers);
            }
        }
        Some(Commands::Update) => {
            downloader::update_yt_dlp(None)?;
        }
        None => {
            // Masaüstü arayüzü başlatılıyor
            let app = App::new(config, dir_given);
            gui::run_gui(app)?;
        }
    }

    Ok(())
}
