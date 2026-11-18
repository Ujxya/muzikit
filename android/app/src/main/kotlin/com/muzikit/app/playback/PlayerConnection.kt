package com.muzikit.app.playback

import android.content.ComponentName
import android.content.Context
import androidx.core.content.ContextCompat
import androidx.media3.common.Player
import androidx.media3.session.MediaController
import androidx.media3.session.SessionToken
import com.google.common.util.concurrent.ListenableFuture
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

/** Arayüzün çalar hakkında bildiği her şey (anlık konum hariç: o sadece ekran açıkken sorulur). */
data class PlayerUi(
    val connected: Boolean = false,
    val mediaId: String? = null,
    val title: String? = null,
    val isPlaying: Boolean = false,
    val durationMs: Long = 0,
)

/**
 * Arayüz ↔ PlaybackService bağlantısı. Etkinlik görünürken (onStart) bağlanır — bu, servisi de ayağa
 * kaldırır — görünmez olunca (onStop) bırakılır: arka planda arayüz tarafında hiçbir dinleyici çalışmaz.
 * Komutlar (çal, sonraki, karıştır…) MusicHub'a gider; burası sadece durumu ve sarma/konumu taşır.
 */
class PlayerConnection(private val context: Context) {

    private var future: ListenableFuture<MediaController>? = null
    private var controller: MediaController? = null
    private val _ui = MutableStateFlow(PlayerUi())
    val ui: StateFlow<PlayerUi> = _ui.asStateFlow()

    private val listener = object : Player.Listener {
        override fun onEvents(player: Player, events: Player.Events) = refresh()
    }

    fun connect() {
        if (future != null) return
        val token = SessionToken(context, ComponentName(context, PlaybackService::class.java))
        val f = MediaController.Builder(context, token).buildAsync()
        future = f
        f.addListener({
            if (future !== f) return@addListener
            val c = runCatching { f.get() }.getOrNull() ?: return@addListener
            controller = c
            c.addListener(listener)
            refresh()
        }, ContextCompat.getMainExecutor(context))
    }

    fun release() {
        controller?.removeListener(listener)
        controller = null
        future?.let { MediaController.releaseFuture(it) }
        future = null
        _ui.value = PlayerUi()
    }

    private fun refresh() {
        val c = controller ?: return
        _ui.value = PlayerUi(
            connected = true,
            mediaId = c.currentMediaItem?.mediaId,
            title = c.mediaMetadata.title?.toString(),
            isPlaying = c.isPlaying,
            durationMs = c.duration.coerceAtLeast(0),
        )
    }

    /** Sadece ekran açıkken, ilerleme çubuğu için sorulur. */
    fun positionMs(): Long = controller?.currentPosition ?: 0

    fun seekTo(ms: Long) { controller?.seekTo(ms) }
}
