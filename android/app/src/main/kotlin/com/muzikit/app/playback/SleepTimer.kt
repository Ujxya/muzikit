package com.muzikit.app.playback

/**
 * Uyku zamanlayıcısı (masaüstü `SleepTimer` / `cycle_sleep`): Kapalı → 15 → 30 → 45 → 60 → 90 dk →
 * şarkı sonu → Kapalı. Süre bitince çalma DURAKLATILIR. Zaman: `SystemClock.elapsedRealtime()` (ms).
 */
sealed interface SleepTimer {
    data object Off : SleepTimer
    data class At(val atMs: Long) : SleepTimer
    data object EndOfTrack : SleepTimer

    companion object {
        val CHOICES_MIN = longArrayOf(15, 30, 45, 60, 90)

        fun cycle(cur: SleepTimer, now: Long): SleepTimer = when (cur) {
            Off -> At(now + CHOICES_MIN[0] * 60_000)
            is At -> {
                // Kalan süreden büyük ilk seçenek; yoksa "şarkı sonu"
                val remaining = (cur.atMs - now).coerceAtLeast(0) / 60_000 + 1
                CHOICES_MIN.firstOrNull { it > remaining }?.let { At(now + it * 60_000) } ?: EndOfTrack
            }
            EndOfTrack -> Off
        }

        /** Paneldeki kısa yazı: "uyku", "28dk", "son". */
        fun shortLabel(t: SleepTimer, now: Long): String = when (t) {
            Off -> "uyku"
            EndOfTrack -> "son"
            is At -> "${((t.atMs - now).coerceAtLeast(0) + 59_999) / 60_000}dk"
        }

        fun longLabel(t: SleepTimer, now: Long): String = when (t) {
            Off -> "kapalı"
            EndOfTrack -> "şarkı sonunda"
            is At -> "${((t.atMs - now).coerceAtLeast(0) + 59_999) / 60_000} dk sonra"
        }
    }
}
