package com.muzikit.app.playback

/**
 * "Ses kısılınca dur, ses geri gelince devam et" kuralı (Android'den bağımsız, birim testli).
 *
 * - Çalarken medya sesi 0'a inerse (veya sessize alınırsa) → duraklat ve bunu hatırla.
 * - Ses 0'dan yukarı çıkınca, duraklatmayı BİZ yaptıysak → devam et.
 * - Kullanıcı arada kendisi oynatırsa / başka şarkı açarsa hatıra silinir (sonradan kendiliğinden başlamasın).
 */
class MutePause {
    enum class Action { NONE, PAUSE, PLAY }

    var pausedByMute = false
        private set

    fun onVolume(volume: Int, muted: Boolean, isPlaying: Boolean): Action {
        val silent = volume <= 0 || muted
        return when {
            silent && isPlaying -> { pausedByMute = true; Action.PAUSE }
            !silent && pausedByMute -> { pausedByMute = false; if (isPlaying) Action.NONE else Action.PLAY }
            else -> Action.NONE
        }
    }

    /** Çalma kendiliğinden (kullanıcı/başka sebep) başladıysa artık "biz durdurduk" denemez. */
    fun onStartedPlaying() { pausedByMute = false }
}
