# youtubedl-android: yt-dlp çıktısını Jackson ile yansıma (reflection) üzerinden okur.
-keep class com.yausername.** { *; }
-keep class com.fasterxml.jackson.** { *; }
-dontwarn com.fasterxml.jackson.**
-dontwarn java.beans.**
-dontwarn org.apache.commons.compress.**
# Python/FFmpeg arşivlerini açan zip kütüphanesi statik başlatmada yansıma kullanıyor; R8 bozmasın.
-keep class org.apache.commons.compress.** { *; }
-keep class org.apache.commons.io.** { *; }
-dontwarn org.apache.commons.**
