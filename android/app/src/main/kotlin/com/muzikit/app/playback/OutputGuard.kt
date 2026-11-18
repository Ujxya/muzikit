package com.muzikit.app.playback

/** Ses çıkışının türü. Sadece kulaklık modunda WIRED ve BLUETOOTH "kulaklık" sayılır. */
enum class OutputKind { WIRED, BLUETOOTH, SPEAKER, OTHER }

/** Bir ses çıkışı. [key] aynı aygıtı yeniden bağlanınca tanımak için (tür + ürün adı). */
data class Output(val key: String, val kind: OutputKind) {
    val isHeadphone: Boolean get() = kind == OutputKind.WIRED || kind == OutputKind.BLUETOOTH
}

/**
 * Kulaklık / Bluetooth kurallarının Android'den bağımsız hâli (birim testli).
 *
 * - Sadece kulaklık modu: kulaklık yokken çalma reddedilir; çalarken son kulaklık giderse duraklatılır.
 * - Kulaklık kopunca durma için ek güvence: ExoPlayer'ın "noisy" duraklatması gelmese bile, çalarken son
 *   kulaklık giderse (ses hoparlöre geçecekse) duraklatılır.
 * - "BT gelince devam et": Bluetooth kulaklık koptuğu için duraklatılan şarkı, AYNI aygıt
 *   [resumeWindowMs] içinde geri gelirse devam eder. Kablolu kulaklıkta kendiliğinden başlamaz.
 */
class OutputGuard(
    private val correlateMs: Long = 3_000,
    private val resumeWindowMs: Long = 5 * 60_000,
) {
    enum class Action { NONE, PAUSE, PLAY }

    private var lastLost: Pair<Output, Long>? = null
    private var lastNoisyPauseAt: Long? = null

    /** Geri gelmesi beklenen Bluetooth aygıtı ve kopma anı. */
    var pending: Pair<String, Long>? = null
        private set

    fun canPlay(headphoneOnly: Boolean, outputs: List<Output>): Boolean =
        !headphoneOnly || outputs.any { it.isHeadphone }

    /** Çıkış(lar) çıkarıldı. [current]: çıkarıldıktan sonra kalanlar. */
    fun onRemoved(removed: List<Output>, current: List<Output>, isPlaying: Boolean, now: Long): Action {
        val lost = removed.firstOrNull { it.isHeadphone } ?: return Action.NONE
        lastLost = lost to now
        if (isPlaying) {
            // Başka kulaklık hâlâ bağlıysa ses ona geçer, durmaya gerek yok.
            if (current.any { it.isHeadphone }) return Action.NONE
            remember(lost, now)
            return Action.PAUSE
        }
        if (lastNoisyPauseAt?.let { now - it <= correlateMs } == true) remember(lost, now)
        return Action.NONE
    }

    /** ExoPlayer "kulaklık çıktı" (noisy) yüzünden duraklattı. */
    fun onNoisyPause(now: Long) {
        lastNoisyPauseAt = now
        val (out, at) = lastLost ?: return
        if (now - at <= correlateMs) remember(out, now)
    }

    private fun remember(out: Output, now: Long) {
        pending = if (out.kind == OutputKind.BLUETOOTH) out.key to now else null
    }

    fun onAdded(added: List<Output>, resumeEnabled: Boolean, isPlaying: Boolean, now: Long): Action {
        val (key, at) = pending ?: return Action.NONE
        if (now - at > resumeWindowMs) { pending = null; return Action.NONE }
        if (added.none { it.key == key }) return Action.NONE
        pending = null
        return if (resumeEnabled && !isPlaying) Action.PLAY else Action.NONE
    }

    /** Çalma başladı (kullanıcı elle başlattı ya da biz devam ettirdik): bekleyen devam iptal. */
    fun onStartedPlaying() { pending = null }
}
