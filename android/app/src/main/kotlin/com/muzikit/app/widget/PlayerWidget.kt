package com.muzikit.app.widget

import android.app.PendingIntent
import android.appwidget.AppWidgetManager
import android.appwidget.AppWidgetProvider
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.widget.RemoteViews
import com.muzikit.app.MainActivity
import com.muzikit.app.R

/**
 * Ana ekran widget'ı. Kendi kendine hiç uyanmaz (updatePeriodMillis=0); PlaybackService şarkı veya
 * çal/duraklat durumu değişince [push] çağırır. Son durum küçük bir ayar dosyasında tutulur ki
 * widget yeniden çizildiğinde (telefon açılışı vb.) servis çalışmasa da doğru görünsün.
 */
class PlayerWidget : AppWidgetProvider() {

    override fun onUpdate(context: Context, manager: AppWidgetManager, ids: IntArray) {
        val p = prefs(context)
        render(context, manager, ids, p.getString(K_TITLE, null), p.getString(K_NUMBER, null), p.getBoolean(K_PLAYING, false))
    }

    companion object {
        private const val K_TITLE = "title"
        private const val K_NUMBER = "number"
        private const val K_PLAYING = "playing"

        private fun prefs(c: Context) = c.getSharedPreferences("widget", Context.MODE_PRIVATE)

        /** Servis çağırır; widget yoksa hiçbir şey yapmaz (ucuz). */
        fun push(context: Context, title: String?, number: String?, playing: Boolean) {
            val p = prefs(context)
            if (p.getString(K_TITLE, null) == title && p.getString(K_NUMBER, null) == number &&
                p.getBoolean(K_PLAYING, false) == playing
            ) return
            p.edit().putString(K_TITLE, title).putString(K_NUMBER, number).putBoolean(K_PLAYING, playing).apply()
            val manager = AppWidgetManager.getInstance(context)
            val ids = manager.getAppWidgetIds(ComponentName(context, PlayerWidget::class.java))
            if (ids.isEmpty()) return
            render(context, manager, ids, title, number, playing)
        }

        private fun render(
            context: Context, manager: AppWidgetManager, ids: IntArray,
            title: String?, number: String?, playing: Boolean,
        ) {
            val v = RemoteViews(context.packageName, R.layout.widget_player)
            v.setTextViewText(R.id.w_title, title ?: "—")
            v.setTextViewText(R.id.w_number, number?.let { "#$it" } ?: "muzikit")
            v.setImageViewResource(R.id.w_play, if (playing) R.drawable.ic_w_pause else R.drawable.ic_w_play)
            v.setOnClickPendingIntent(R.id.w_prev, action(context, WidgetActionReceiver.PREV, 1))
            v.setOnClickPendingIntent(R.id.w_play, action(context, WidgetActionReceiver.TOGGLE, 2))
            v.setOnClickPendingIntent(R.id.w_next, action(context, WidgetActionReceiver.NEXT, 3))
            v.setOnClickPendingIntent(
                R.id.w_root,
                PendingIntent.getActivity(
                    context, 4, Intent(context, MainActivity::class.java),
                    PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
                ),
            )
            manager.updateAppWidget(ids, v)
        }

        private fun action(context: Context, what: String, code: Int): PendingIntent =
            PendingIntent.getBroadcast(
                context, code,
                Intent(context, WidgetActionReceiver::class.java).setAction(what),
                PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
            )
    }
}
