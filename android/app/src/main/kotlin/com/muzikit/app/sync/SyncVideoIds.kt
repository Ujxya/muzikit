package com.muzikit.app.sync

import android.content.Context

/** İndiricinin indirdiği dosya adı → YouTube ID (defter taranırken `videoId` sütununa yazılır). */
object SyncVideoIds {
    private fun prefs(c: Context) = c.getSharedPreferences("sync_videoids", Context.MODE_PRIVATE)

    fun note(context: Context, fileName: String, videoId: String) {
        prefs(context).edit().putString(fileName, videoId).apply()
    }

    fun all(context: Context): Map<String, String> =
        prefs(context).all.mapNotNull { (k, v) -> (v as? String)?.let { k to it } }.toMap()
}
