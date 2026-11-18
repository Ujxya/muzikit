package com.muzikit.app.download

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.IBinder
import android.os.SystemClock
import androidx.core.app.NotificationCompat
import androidx.core.app.ServiceCompat
import com.muzikit.app.MainActivity
import com.muzikit.app.R
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch

/**
 * İndirme / yt-dlp güncelleme için ön plan servisi (tür: dataSync). Ekran kapansa da iş sürer;
 * iş bitince servis kendini kapatır. Arka planda başka hiçbir şey yapmaz.
 */
class DownloadService : Service() {

    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main)
    private var running = false
    private var lastNotify = 0L

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        ensureChannel()
        ServiceCompat.startForeground(
            this, NOTIF_ID, notification("hazırlanıyor…", -1f),
            ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC,
        )
        if (running) return START_NOT_STICKY // kuyruğa eklenen iş, çalışan döngü tarafından alınır
        running = true
        val update = intent?.action == ACTION_UPDATE
        scope.launch {
            try {
                if (update) DownloadCenter.runUpdate(applicationContext)
                DownloadCenter.runQueue(applicationContext) { label, p -> notifyProgress(label, p) }
            } finally {
                running = false
                ServiceCompat.stopForeground(this@DownloadService, ServiceCompat.STOP_FOREGROUND_REMOVE)
                stopSelf()
            }
        }
        return START_NOT_STICKY
    }

    override fun onDestroy() {
        scope.cancel()
        super.onDestroy()
    }

    /** Bildirim en fazla saniyede ~2 kez güncellenir. */
    private fun notifyProgress(label: String, p: Float) {
        val now = SystemClock.elapsedRealtime()
        if (now - lastNotify < 500) return
        lastNotify = now
        getSystemService(NotificationManager::class.java).notify(NOTIF_ID, notification(label, p))
    }

    private fun notification(text: String, p: Float): Notification {
        val open = PendingIntent.getActivity(
            this, 10, Intent(this, MainActivity::class.java),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )
        return NotificationCompat.Builder(this, CHANNEL)
            .setSmallIcon(R.drawable.ic_w_next)
            .setContentTitle("muzikit: indiriliyor")
            .setContentText(text)
            .setOnlyAlertOnce(true)
            .setOngoing(true)
            .setSilent(true)
            .setContentIntent(open)
            .apply { if (p >= 0f) setProgress(100, (p * 100).toInt(), false) else setProgress(0, 0, true) }
            .build()
    }

    private fun ensureChannel() {
        val nm = getSystemService(NotificationManager::class.java)
        if (nm.getNotificationChannel(CHANNEL) == null) {
            nm.createNotificationChannel(
                NotificationChannel(CHANNEL, "İndirmeler", NotificationManager.IMPORTANCE_LOW).apply {
                    description = "YouTube'dan indirme ilerlemesi"
                    setShowBadge(false)
                }
            )
        }
    }

    companion object {
        const val ACTION_UPDATE = "com.muzikit.app.download.UPDATE"
        private const val CHANNEL = "downloads"
        private const val NOTIF_ID = 2001
    }
}
