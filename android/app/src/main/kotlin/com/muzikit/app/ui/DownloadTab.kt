package com.muzikit.app.ui

import android.Manifest
import android.content.ClipboardManager
import android.content.pm.PackageManager
import android.os.Build
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.core.content.ContextCompat
import com.muzikit.app.MusicHub
import com.muzikit.app.download.DownloadCenter
import androidx.compose.ui.focus.onFocusChanged
import com.muzikit.app.download.DownloadState
import com.muzikit.app.ui.theme.MuzikitColors

/** YouTube / YouTube Music linkinden Opus indirme (masaüstü "İndir" ekranı). */
@Composable
fun DownloadTab(m: ScreenModel, d: DownloadState) {
    val context = LocalContext.current
    var text by rememberSaveable { mutableStateOf("") }
    val notifPermission = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { }
    val focusManager = androidx.compose.ui.platform.LocalFocusManager.current

    fun start() {
        // İlerleme bildirimi için (Android 13+); reddedilse de indirme çalışır.
        if (Build.VERSION.SDK_INT >= 33 &&
            ContextCompat.checkSelfPermission(context, Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED
        ) notifPermission.launch(Manifest.permission.POST_NOTIFICATIONS)
        DownloadCenter.submit(context.applicationContext, text)
        text = ""
        focusManager.clearFocus() // klavye kapansın, playlist sorusu görünsün
    }

    Column(Modifier.fillMaxSize().padding(horizontal = 16.dp)) {
        Text(
            "kişisel kullanım içindir; YouTube'dan indirme kullanım şartlarına aykırı olabilir.",
            style = MaterialTheme.typography.labelSmall, color = MuzikitColors.DarkGray,
            modifier = Modifier.padding(top = 6.dp, bottom = 6.dp),
        )
        // Playlist sorusu
        d.pendingPlaylist?.let { (_, single) ->
            Column(
                Modifier.fillMaxWidth().padding(bottom = 10.dp).border(1.dp, MuzikitColors.Gray)
                    .background(MuzikitColors.Panel).padding(10.dp),
            ) {
                Text("bu link bir playlist. ne indirilsin?", style = MaterialTheme.typography.bodyMedium, color = MuzikitColors.White)
                Spacer(Modifier.height(8.dp))
                Row {
                    Btn("tamamı") { DownloadCenter.answerPlaylist(context.applicationContext, true) }
                    Spacer(Modifier.width(8.dp))
                    if (single != null) {
                        Btn("sadece bu video") { DownloadCenter.answerPlaylist(context.applicationContext, false) }
                        Spacer(Modifier.width(8.dp))
                    }
                    Btn("iptal") { DownloadCenter.answerPlaylist(context.applicationContext, null) }
                }
            }
        }

        when {
            m.lib.folder == null -> Note("önce müzik klasörünü seç", "klasör seç", m.pickFolder)
            !m.lib.writable -> Note(
                "indirilen şarkıları klasöre kaydetmek için yazma izni gerekiyor. aynı klasörü (Music/Müzik) yeniden seç.",
                "klasöre yazma izni ver", m.pickFolder,
            )
            else -> {
                // Link kutusu
                Box(
                    Modifier.fillMaxWidth().heightIn(min = 44.dp).border(1.dp, MuzikitColors.DarkGray)
                        .padding(horizontal = 10.dp, vertical = 10.dp),
                ) {
                    if (text.isEmpty()) {
                        Text("youtube / youtube music linki (birden fazla: boşlukla ayır)",
                            style = MaterialTheme.typography.bodySmall, color = MuzikitColors.DarkGray)
                    }
                    BasicTextField(
                        value = text, onValueChange = { text = it.take(20_000) },
                        textStyle = MaterialTheme.typography.bodySmall.copy(color = MuzikitColors.White),
                        cursorBrush = SolidColor(MuzikitColors.White),
                        maxLines = 4,
                        // Link yazarken harfler/"/" klavye kısayolu sayılmasın
                        modifier = Modifier.fillMaxWidth()
                            .onFocusChanged { MusicHub.textInputFocused = it.isFocused },
                    )
                }
                Spacer(Modifier.height(8.dp))
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Btn("yapıştır") {
                        val cm = context.getSystemService(ClipboardManager::class.java)
                        cm.primaryClip?.getItemAt(0)?.coerceToText(context)?.toString()?.let {
                            text = (if (text.isBlank()) "" else "$text ") + it.trim()
                        }
                    }
                    Spacer(Modifier.width(8.dp))
                    Btn("indir", enabled = text.isNotBlank()) { start() }
                    Spacer(Modifier.weight(1f))
                    if (d.busy) Btn("iptal") { DownloadCenter.cancel() }
                }
            }
        }

        // Durum + ilerleme
        if (d.busy || d.updating) {
            Spacer(Modifier.height(10.dp))
            Text(
                if (d.updating) "yt-dlp güncelleniyor…" else (d.current.ifEmpty { "hazırlanıyor…" } +
                    if (d.queued > 0) "   (+${d.queued} iş)" else ""),
                style = MaterialTheme.typography.bodySmall, color = MuzikitColors.White, maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            Spacer(Modifier.height(4.dp))
            Box(Modifier.fillMaxWidth().height(3.dp).background(MuzikitColors.DarkGray)) {
                if (d.progress >= 0f) {
                    Box(Modifier.fillMaxWidth(d.progress.coerceIn(0f, 1f)).height(3.dp).background(MuzikitColors.White))
                }
            }
        }

        // yt-dlp sürümü
        Row(Modifier.fillMaxWidth().padding(top = 12.dp, bottom = 4.dp), verticalAlignment = Alignment.CenterVertically) {
            Text("yt-dlp: ${d.ytdlpVersion ?: "henüz başlatılmadı"}",
                style = MaterialTheme.typography.bodySmall, color = MuzikitColors.Gray, modifier = Modifier.weight(1f),
                maxLines = 1, overflow = TextOverflow.Ellipsis)
            Btn("güncelle", enabled = !d.busy && !d.updating) { DownloadCenter.requestUpdate(context.applicationContext) }
        }

        // Kayıt
        Row(Modifier.fillMaxWidth().padding(top = 8.dp), verticalAlignment = Alignment.CenterVertically) {
            Text("kayıt", style = MaterialTheme.typography.labelLarge, color = MuzikitColors.White, modifier = Modifier.weight(1f))
            if (d.log.isNotEmpty()) Text("temizle", style = MaterialTheme.typography.labelSmall, color = MuzikitColors.Gray,
                modifier = Modifier.clickable { DownloadCenter.clearLog() }.padding(6.dp))
        }
        Box(Modifier.fillMaxWidth().height(1.dp).background(MuzikitColors.DarkGray))
        val logState = rememberLazyListState()
        LaunchedEffect(d.log.size) { if (d.log.isNotEmpty()) logState.scrollToItem(d.log.lastIndex) }
        LazyColumn(Modifier.fillMaxWidth().weight(1f), state = logState) {
            items(d.log) { line ->
                Text(line, style = MaterialTheme.typography.bodySmall,
                    color = if (line.startsWith("✓") || line.startsWith("✨")) MuzikitColors.White else MuzikitColors.Gray,
                    modifier = Modifier.padding(vertical = 2.dp))
            }
        }
    }
}

@Composable
private fun Note(text: String, button: String, onClick: () -> Unit) {
    Column(Modifier.fillMaxWidth().padding(vertical = 12.dp)) {
        Text(text, style = MaterialTheme.typography.bodyMedium, color = MuzikitColors.Gray)
        Spacer(Modifier.height(10.dp))
        Btn(button, onClick = onClick)
    }
}

@Composable
private fun Btn(label: String, enabled: Boolean = true, onClick: () -> Unit) {
    Text(
        label,
        style = MaterialTheme.typography.labelLarge,
        color = if (enabled) MuzikitColors.White else MuzikitColors.DarkGray,
        maxLines = 1,
        modifier = Modifier
            .border(1.dp, if (enabled) MuzikitColors.Gray else MuzikitColors.DarkGray)
            .background(Color.Transparent)
            .clickable(enabled = enabled, onClick = onClick)
            .padding(horizontal = 10.dp, vertical = 6.dp),
    )
}
