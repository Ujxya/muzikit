package com.muzikit.app.widget

import android.content.BroadcastReceiver
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.os.Handler
import android.os.Looper
import androidx.core.content.ContextCompat
import androidx.media3.session.MediaController
import androidx.media3.session.SessionToken
import com.muzikit.app.MusicHub
import com.muzikit.app.playback.PlaybackService

/**
 * Widget düğmeleri. Servis çalışıyorsa doğrudan MusicHub'a; çalışmıyorsa önce servise bağlanıp
 * (servisi başlatır) kütüphane hazır olunca eylemi uygular.
 */
class WidgetActionReceiver : BroadcastReceiver() {

    override fun onReceive(context: Context, intent: Intent) {
        val what = intent.action ?: return
        val app = context.applicationContext
        MusicHub.init(app)
        if (MusicHub.hasPlayer()) {
            MusicHub.whenLibraryReady { run(what) }
            return
        }
        val pending = goAsync()
        val token = SessionToken(app, ComponentName(app, PlaybackService::class.java))
        val future = MediaController.Builder(app, token).buildAsync()
        future.addListener({
            MusicHub.whenLibraryReady {
                run(what)
                // Bağlantıyı kısa süre sonra bırak: çalıyorsa servis ön planda kalır, değilse kendiliğinden kapanır.
                Handler(Looper.getMainLooper()).postDelayed({ MediaController.releaseFuture(future) }, 3_000)
                pending.finish()
            }
        }, ContextCompat.getMainExecutor(app))
    }

    private fun run(what: String) {
        when (what) {
            TOGGLE -> MusicHub.togglePlay()
            NEXT -> MusicHub.next(manual = true)
            PREV -> MusicHub.prev()
        }
    }

    companion object {
        const val TOGGLE = "com.muzikit.app.widget.TOGGLE"
        const val NEXT = "com.muzikit.app.widget.NEXT"
        const val PREV = "com.muzikit.app.widget.PREV"
    }
}
