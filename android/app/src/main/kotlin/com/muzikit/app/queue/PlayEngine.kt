package com.muzikit.app.queue

import kotlin.random.Random

/** Döngü: Kapalı → Liste → Tek (masaüstü `LoopMode`). */
enum class LoopMode(val label: String) {
    OFF("kapalı"), ALL("liste"), ONE("tek");

    fun next(): LoopMode = when (this) { OFF -> ALL; ALL -> ONE; ONE -> OFF }

    companion object {
        fun fromKey(k: String?): LoopMode = entries.firstOrNull { it.name == k } ?: OFF
    }
}

/** "Önceki"ye basınca ne olacağı. */
sealed interface PrevAction {
    data object SeekToStart : PrevAction
    data class Play(val file: String) : PrevAction
    data object Nothing : PrevAction
}

/**
 * Çalma sırası kararları — masaüstü `src/app.rs` (`play_file`, `play_next`, `play_prev`,
 * `next_shuffled`, `enqueue`, `queue_move`) mantığının Android'den bağımsız karşılığı.
 * Şarkılar dosya adıyla anılır. Bu sınıf ses çalmaz; sadece "sıradaki hangisi" sorusunu cevaplar.
 *
 * `ctx` (bağlam): sıralı çalmada kullanılan liste. Çağıran taraf masaüstündeki gibi verir:
 * çalan şarkı görünür (filtrelenmiş) listedeyse o liste, değilse tüm kütüphane.
 */
class PlayEngine(private val rng: Random = Random.Default) {

    var playing: String? = null
        private set
    var shuffle = false
        private set
    var loop = LoopMode.OFF

    private val history = ArrayList<String>()
    private val _queue = ArrayList<String>()
    val queue: List<String> get() = _queue
    private val _recent = ArrayList<String>()
    val recent: List<String> get() = _recent

    /** Karıştırma torbası; sondan çekilir. */
    private val bag = ArrayList<String>()

    /** Bir şarkının çalmaya başladığını kaydeder (geçmiş, son çalınanlar, torba). */
    fun play(file: String, pushHistory: Boolean = true) {
        val prev = playing
        if (pushHistory && prev != null && prev != file) {
            history += prev
            if (history.size > HISTORY_MAX) history.removeAt(0)
        }
        _recent.remove(file)
        _recent.add(0, file)
        while (_recent.size > RECENT_MAX) _recent.removeAt(_recent.lastIndex)
        playing = file
        bag.remove(file)
    }

    /**
     * Sıradaki şarkı. [manual]: kullanıcı "sonraki"ye bastı (liste sonunda başa döner);
     * değilse şarkı kendiliğinden bitti (döngü kapalıysa liste sonunda null = dur).
     * Dönen şarkı için ardından [play] çağrılmalı.
     */
    fun next(manual: Boolean, ctx: List<String>, exists: (String) -> Boolean): String? {
        val cur = playing
        if (!manual && loop == LoopMode.ONE && cur != null) return cur
        // Kuyruk her zaman önceliklidir
        while (_queue.isNotEmpty()) {
            val f = _queue.removeAt(0)
            if (exists(f)) return f
        }
        if (ctx.isEmpty()) return null
        if (shuffle) return nextShuffled(ctx)
        val pos = cur?.let { ctx.indexOf(it) } ?: -1
        return when {
            pos < 0 -> ctx[0]
            pos + 1 < ctx.size -> ctx[pos + 1]
            manual || loop == LoopMode.ALL -> ctx[0]
            else -> null
        }
    }

    private fun nextShuffled(ctx: List<String>): String? {
        if (bag.isEmpty()) {
            bag.addAll(ctx)
            // Fisher–Yates: tüm şarkılar bitmeden tekrar yok
            for (i in bag.indices.reversed()) {
                if (i == 0) break
                val j = rng.nextInt(i + 1)
                val t = bag[i]; bag[i] = bag[j]; bag[j] = t
            }
            // Yeni torbanın ilki (sondan çekilir) az önce çalan olmasın
            if (bag.size > 1 && bag.last() == playing) {
                val t = bag[0]; bag[0] = bag[bag.lastIndex]; bag[bag.lastIndex] = t
            }
        }
        return if (bag.isEmpty()) null else bag.removeAt(bag.lastIndex)
    }

    /** "Önceki": 3 sn'den fazla çaldıysa başa sar; değilse geçmişten, o da yoksa listede bir önceki. */
    fun prev(positionMs: Long, ctx: List<String>, exists: (String) -> Boolean): PrevAction {
        if (playing != null && positionMs > PREV_RESTART_MS) return PrevAction.SeekToStart
        while (history.isNotEmpty()) {
            val f = history.removeAt(history.lastIndex)
            if (exists(f)) return PrevAction.Play(f)
        }
        if (ctx.isEmpty()) return PrevAction.Nothing
        val pos = playing?.let { ctx.indexOf(it) } ?: -1
        return PrevAction.Play(if (pos <= 0) ctx.last() else ctx[pos - 1])
    }

    fun setShuffle(on: Boolean) {
        shuffle = on
        bag.clear()
    }

    fun enqueue(file: String, front: Boolean) {
        if (front) _queue.add(0, file) else _queue.add(file)
    }

    fun queueMove(idx: Int, up: Boolean) {
        if (up && idx > 0 && idx < _queue.size) {
            val t = _queue[idx]; _queue[idx] = _queue[idx - 1]; _queue[idx - 1] = t
        } else if (!up && idx >= 0 && idx + 1 < _queue.size) {
            val t = _queue[idx]; _queue[idx] = _queue[idx + 1]; _queue[idx + 1] = t
        }
    }

    fun queueRemove(idx: Int) {
        if (idx in _queue.indices) _queue.removeAt(idx)
    }

    fun queueClear() = _queue.clear()

    /** Yeniden taramadan sonra silinmiş dosyaları kuyruk/geçmiş/son çalınanlardan temizle. */
    fun prune(exists: (String) -> Boolean) {
        _queue.retainAll(exists)
        history.retainAll(exists)
        _recent.retainAll(exists)
        bag.clear()
    }

    /** Eşitlemede bir dosyanın adı değişti (numara kayması): kuyruk/geçmiş/son çalınanlar/çalan yeni adı alır. */
    fun renameFile(old: String, new: String) {
        fun MutableList<String>.swap() = replaceAll { if (it == old) new else it }
        _queue.swap(); history.swap(); _recent.swap(); bag.swap()
        if (playing == old) playing = new
    }

    /** Kalıcı ayardan son çalınanları geri yükle. */
    fun restoreRecent(files: List<String>) {
        _recent.clear()
        _recent.addAll(files.distinct().take(RECENT_MAX))
    }

    companion object {
        const val HISTORY_MAX = 200
        const val RECENT_MAX = 30
        const val PREV_RESTART_MS = 3000L
    }
}
