package com.muzikit.app.ui

import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Slider
import androidx.compose.material3.SliderDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.repeatOnLifecycle
import com.muzikit.app.LibraryState
import com.muzikit.app.MusicHub
import com.muzikit.app.PlayState
import com.muzikit.app.Settings
import com.muzikit.app.library.LibraryReport
import com.muzikit.app.playback.SleepTimer
import com.muzikit.app.Toast
import com.muzikit.app.UiEvent
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import com.muzikit.app.ViewState
import com.muzikit.app.library.Song
import com.muzikit.app.playback.PlayerUi
import com.muzikit.app.queue.LoopMode
import com.muzikit.app.ui.theme.MuzikitColors
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

/** Ekranın ihtiyaç duyduğu her şey (durum + eylemler). */
class ScreenModel(
    val lib: LibraryState,
    val visible: List<Song>,
    val view: ViewState,
    val favorites: Set<String>,
    val play: PlayState,
    val player: PlayerUi,
    val toast: Toast?,
    val settings: Settings,
    val sleep: SleepTimer,
    val report: LibraryReport?,
    val download: com.muzikit.app.download.DownloadState,
    val sync: com.muzikit.app.sync.SyncUiState,
    val position: () -> Long,
    val seekTo: (Long) -> Unit,
    val pickFolder: () -> Unit,
)

private enum class Tab(val label: String) { LIST("liste"), QUEUE("kuyruk"), RECENT("son"), DOWNLOAD("indir"), SETTINGS("ayarlar") }

@Composable
fun MainScreen(m: ScreenModel) {
    var tab by rememberSaveable { mutableStateOf(Tab.LIST) }
    val listState = rememberLazyListState()
    val scope = rememberCoroutineScope()
    val searchFocus = remember { FocusRequester() }
    val reveal: () -> Unit = {
        val i = m.visible.indexOfFirst { it.file == m.play.playing }
        if (i >= 0) {
            tab = Tab.LIST
            scope.launch { listState.scrollToItem(i) }
        } else if (m.play.playing != null) {
            MusicHub.toast("çalan şarkı bu listede yok")
        }
    }
    // Klavye kısayollarından gelen istekler (sadece ekran görünürken toplanır)
    val lifecycle = LocalLifecycleOwner.current.lifecycle
    LaunchedEffect(lifecycle) {
        lifecycle.repeatOnLifecycle(Lifecycle.State.STARTED) {
            MusicHub.events.collect { e ->
                when (e) {
                    UiEvent.FOCUS_SEARCH -> {
                        tab = Tab.LIST
                        delay(50)
                        runCatching { searchFocus.requestFocus() }
                    }
                    UiEvent.REVEAL_PLAYING -> reveal()
                }
            }
        }
    }

    val ticker = rememberVisualTicker(m.settings.motion, m.settings.analyzer, m.player.isPlaying)
    Box(Modifier.fillMaxSize()) {
    CyberBackground(motion = m.settings.motion, ticker = ticker)
    Column(
        Modifier
            .fillMaxSize()
            .safeDrawingPadding()
    ) {
        Header(m)
        TabRow(tab, queueSize = m.play.queue.size) { tab = it }
        Box(Modifier.weight(1f).fillMaxWidth()) {
            when (tab) {
                Tab.LIST -> LibraryTab(m, listState, searchFocus)
                Tab.QUEUE -> QueueTab(m)
                Tab.RECENT -> RecentTab(m)
                Tab.DOWNLOAD -> DownloadTab(m, m.download)
                Tab.SETTINGS -> SettingsTab(m)
            }
            ToastLine(m.toast, Modifier.align(Alignment.BottomCenter))
        }
        // Alttaki çalar paneli her sekmede aynı.
        PlayerPanel(m, ticker, onTitleClick = reveal)
    }
    }
}

// ── Üst kısım ────────────────────────────────────────────

@Composable
private fun Header(m: ScreenModel) {
    Row(
        Modifier.fillMaxWidth().padding(start = 16.dp, end = 16.dp, top = 8.dp, bottom = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text("muzikit", style = MaterialTheme.typography.titleMedium, color = MuzikitColors.White)
        Spacer(Modifier.width(10.dp))
        if (m.lib.songs.isNotEmpty()) {
            val n = m.visible.size
            val total = m.lib.songs.size
            Text(
                if (n == total) "$total şarkı" else "$n / $total",
                style = MaterialTheme.typography.bodySmall, color = MuzikitColors.Gray,
                maxLines = 1,
            )
        }
        Spacer(Modifier.weight(1f))
        if (m.lib.folder != null) SmallButton("klasör", m.pickFolder)
    }
}

@Composable
private fun TabRow(selected: Tab, queueSize: Int, onSelect: (Tab) -> Unit) {
    Row(Modifier.fillMaxWidth().padding(horizontal = 8.dp)) {
        Tab.entries.forEach { t ->
            val on = t == selected
            val label = if (t == Tab.QUEUE && queueSize > 0) "${t.label}·$queueSize" else t.label
            Column(
                Modifier
                    .weight(1f)
                    .clickable { onSelect(t) }
                    .padding(vertical = 8.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
            ) {
                Text(
                    label, style = MaterialTheme.typography.labelLarge,
                    color = if (on) MuzikitColors.White else MuzikitColors.Gray, maxLines = 1,
                )
                Spacer(Modifier.height(4.dp))
                Box(
                    Modifier.width(28.dp).height(1.dp)
                        .background(if (on) MuzikitColors.White else Color.Transparent)
                )
            }
        }
    }
}

@Composable
private fun SmallButton(label: String, onClick: () -> Unit, active: Boolean = true) {
    Text(
        label,
        style = MaterialTheme.typography.labelLarge,
        color = if (active) MuzikitColors.White else MuzikitColors.Gray,
        maxLines = 1,
        modifier = Modifier
            .border(1.dp, if (active) MuzikitColors.Gray else MuzikitColors.DarkGray)
            .clickable(onClick = onClick)
            .padding(horizontal = 10.dp, vertical = 6.dp),
    )
}

@Composable
private fun CenterNote(text: String, button: String? = null, onClick: () -> Unit = {}) {
    Column(
        Modifier.fillMaxSize().padding(24.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.Center,
    ) {
        Text(text, style = MaterialTheme.typography.bodyMedium, color = MuzikitColors.Gray)
        if (button != null) {
            Spacer(Modifier.height(16.dp))
            SmallButton(button, onClick)
        }
    }
}

@Composable
private fun ToastLine(toast: Toast?, modifier: Modifier) {
    if (toast == null) return
    LaunchedEffect(toast.id) {
        delay(1800)
        MusicHub.clearToast(toast.id)
    }
    Text(
        toast.text,
        style = MaterialTheme.typography.bodySmall,
        color = MuzikitColors.White,
        modifier = modifier
            .padding(8.dp)
            .border(1.dp, MuzikitColors.DarkGray)
            .background(MuzikitColors.Black)
            .padding(horizontal = 12.dp, vertical = 6.dp),
    )
}

// ── Liste sekmesi ────────────────────────────────────────

@Composable
private fun LibraryTab(m: ScreenModel, listState: LazyListState, searchFocus: FocusRequester) {
    when {
        m.lib.folder == null -> CenterNote("müzik klasörünü seç", "klasör seç", m.pickFolder)
        m.lib.loading -> CenterNote("taranıyor…")
        m.lib.error != null -> CenterNote(m.lib.error, "başka klasör seç", m.pickFolder)
        m.lib.songs.isEmpty() -> CenterNote("bu klasörde .opus yok", "başka klasör seç", m.pickFolder)
        else -> Column(Modifier.fillMaxSize()) {
            SearchRow(m.view, searchFocus)
            if (m.visible.isEmpty()) {
                CenterNote(if (m.view.onlyFavorites && m.view.query.isBlank()) "henüz favori yok" else "sonuç yok")
            } else {
                SongList(m.visible, m.play.playing, m.favorites, listState)
            }
        }
    }
}

@Composable
private fun SearchRow(view: ViewState, focus: FocusRequester) {
    val focusManager = LocalFocusManager.current
    Row(
        Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(
            Modifier
                .weight(1f)
                .height(38.dp) // yazı girilince kutu boyu değişmesin
                .border(1.dp, MuzikitColors.DarkGray)
                .padding(horizontal = 10.dp),
            contentAlignment = Alignment.CenterStart,
        ) {
            if (view.query.isEmpty()) {
                Text("ara…", style = MaterialTheme.typography.bodyMedium, color = MuzikitColors.DarkGray)
            }
            BasicTextField(
                value = view.query,
                onValueChange = MusicHub::setQuery,
                singleLine = true,
                textStyle = MaterialTheme.typography.bodyMedium.copy(color = MuzikitColors.White),
                cursorBrush = SolidColor(MuzikitColors.White),
                keyboardOptions = KeyboardOptions(imeAction = ImeAction.Search),
                keyboardActions = androidx.compose.foundation.text.KeyboardActions(onSearch = { focusManager.clearFocus() }),
                modifier = Modifier.fillMaxWidth().focusRequester(focus)
                    .onFocusChanged { MusicHub.textInputFocused = it.isFocused }
                    .padding(end = if (view.query.isEmpty()) 0.dp else 24.dp),
            )
            if (view.query.isNotEmpty()) {
                Box(
                    Modifier.align(Alignment.CenterEnd).size(24.dp).clickable { MusicHub.setQuery("") },
                    contentAlignment = Alignment.Center,
                ) { GlyphIcon(Glyph.Close, MuzikitColors.Gray, 14.dp) }
            }
        }
        Spacer(Modifier.width(8.dp))
        SmallButton(view.sort.label, MusicHub::cycleSort)
        Spacer(Modifier.width(8.dp))
        Box(
            Modifier
                .size(38.dp)
                .border(1.dp, if (view.onlyFavorites) MuzikitColors.Gray else MuzikitColors.DarkGray)
                .clickable(onClick = MusicHub::toggleOnlyFavorites),
            contentAlignment = Alignment.Center,
        ) {
            GlyphIcon(
                if (view.onlyFavorites) Glyph.Star else Glyph.StarOutline,
                if (view.onlyFavorites) MuzikitColors.White else MuzikitColors.Gray, 18.dp,
            )
        }
    }
}

@Composable
private fun SongList(songs: List<Song>, current: String?, favorites: Set<String>, state: LazyListState) {
    LazyColumn(Modifier.fillMaxSize(), state = state) {
        itemsIndexed(songs, key = { _, s -> s.file }, contentType = { _, _ -> 0 }) { _, s ->
            SongRow(
                s, current = s.file == current, favorite = s.file in favorites,
                onClick = { MusicHub.playFile(s.file) },
            )
        }
    }
}

/** Dokun: çal. Uzun bas: sıradaki yap / sıraya ekle / favori. Sağdaki yıldız: favori. */
@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun SongRow(
    s: Song,
    current: Boolean,
    favorite: Boolean,
    onClick: () -> Unit,
    trailing: (@Composable () -> Unit)? = null,
) {
    var menu by remember { mutableStateOf(false) }
    Box {
        Row(
            Modifier
                .fillMaxWidth()
                .background(if (current) MuzikitColors.Card else Color.Transparent)
                .combinedClickable(onClick = onClick, onLongClick = { menu = true })
                .padding(start = 16.dp, end = 4.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(
                s.numberLabel,
                style = MaterialTheme.typography.bodySmall,
                color = if (current) MuzikitColors.White else MuzikitColors.DarkGray,
            )
            Spacer(Modifier.width(12.dp))
            Text(
                s.title,
                style = MaterialTheme.typography.bodyMedium,
                color = if (current) MuzikitColors.White else MuzikitColors.Gray,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f).padding(vertical = 11.dp),
            )
            if (trailing != null) trailing() else Box(
                Modifier.size(40.dp).clickable { MusicHub.toggleFavorite(s.file) },
                contentAlignment = Alignment.Center,
            ) {
                GlyphIcon(
                    if (favorite) Glyph.Star else Glyph.StarOutline,
                    if (favorite) MuzikitColors.White else MuzikitColors.DarkGray, 16.dp,
                )
            }
        }
        DropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
            MenuItem("sıradaki yap") { MusicHub.enqueue(s.file, front = true); menu = false }
            MenuItem("sıraya ekle") { MusicHub.enqueue(s.file, front = false); menu = false }
            MenuItem(if (favorite) "favoriden çıkar" else "favorilere ekle") {
                MusicHub.toggleFavorite(s.file); menu = false
            }
        }
    }
}

@Composable
private fun MenuItem(text: String, onClick: () -> Unit) {
    DropdownMenuItem(
        text = { Text(text, style = MaterialTheme.typography.bodyMedium, color = MuzikitColors.White) },
        onClick = onClick,
    )
}

// ── Kuyruk ve son çalınanlar ─────────────────────────────

@Composable
private fun QueueTab(m: ScreenModel) {
    val q = m.play.queue
    if (q.isEmpty()) {
        CenterNote("kuyruk boş\n\nlistede bir şarkıya uzun bas → \"sıraya ekle\"")
        return
    }
    Column(Modifier.fillMaxSize()) {
        Row(
            Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 6.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text("sıradaki ${q.size} şarkı", style = MaterialTheme.typography.bodySmall, color = MuzikitColors.Gray)
            Spacer(Modifier.weight(1f))
            SmallButton("temizle", MusicHub::queueClear)
        }
        LazyColumn(Modifier.fillMaxSize()) {
            itemsIndexed(q, key = { i, f -> "$i:$f" }) { i, f ->
                val s = MusicHub.song(f) ?: return@itemsIndexed
                SongRow(
                    s, current = false, favorite = f in m.favorites,
                    onClick = { MusicHub.queueRemove(i); MusicHub.playFile(f) },
                    trailing = {
                        Row {
                            IconBtn(Glyph.Up, i > 0) { MusicHub.queueMove(i, up = true) }
                            IconBtn(Glyph.Down, i < q.lastIndex) { MusicHub.queueMove(i, up = false) }
                            IconBtn(Glyph.Close, true) { MusicHub.queueRemove(i) }
                        }
                    },
                )
            }
        }
    }
}

@Composable
private fun RecentTab(m: ScreenModel) {
    val r = m.play.recent
    if (r.isEmpty()) { CenterNote("henüz bir şey çalınmadı"); return }
    LazyColumn(Modifier.fillMaxSize()) {
        itemsIndexed(r, key = { _, f -> f }) { _, f ->
            val s = MusicHub.song(f) ?: return@itemsIndexed
            SongRow(s, current = f == m.play.playing, favorite = f in m.favorites, onClick = { MusicHub.playFile(f) })
        }
    }
}

@Composable
private fun IconBtn(glyph: Glyph, enabled: Boolean, onClick: () -> Unit) {
    Box(
        Modifier.size(40.dp).clickable(enabled = enabled, onClick = onClick),
        contentAlignment = Alignment.Center,
    ) { GlyphIcon(glyph, if (enabled) MuzikitColors.Gray else MuzikitColors.DarkGray, 16.dp) }
}

// ── Çalar paneli (her ekranda aynı) ──────────────────────

private fun fmt(ms: Long): String {
    val s = (ms / 1000).coerceAtLeast(0)
    return "%d:%02d".format(s / 60, s % 60)
}

@Composable
private fun PlayerPanel(m: ScreenModel, ticker: VisualTicker, onTitleClick: () -> Unit) {
    val p = m.player
    var position by remember { mutableLongStateOf(0L) }
    var dragging by remember { mutableStateOf(false) }
    var dragValue by remember { mutableFloatStateOf(0f) }

    // İlerleme sayacı SADECE ekran görünürken ve çalarken döner; arka planda tamamen durur.
    val lifecycle = LocalLifecycleOwner.current.lifecycle
    LaunchedEffect(p.isPlaying, p.mediaId, lifecycle) {
        position = m.position()
        if (!p.isPlaying) return@LaunchedEffect
        lifecycle.repeatOnLifecycle(Lifecycle.State.STARTED) {
            while (true) {
                position = m.position()
                delay(500)
            }
        }
    }

    val current = m.play.playing
    val hasMedia = p.mediaId != null
    Column(
        Modifier
            .fillMaxWidth()
            .border(1.dp, MuzikitColors.DarkGray)
            .background(MuzikitColors.Panel)
            .padding(horizontal = 12.dp, vertical = 6.dp)
    ) {
        if (m.settings.analyzer || m.settings.motion) {
            Row(
                Modifier.fillMaxWidth().padding(top = 4.dp, bottom = 2.dp),
                verticalAlignment = Alignment.Bottom,
            ) {
                if (m.settings.motion) {
                    Radar(ticker, modifier = Modifier.size(40.dp))
                    Spacer(Modifier.width(12.dp))
                }
                if (m.settings.analyzer) {
                    Column(Modifier.weight(1f)) {
                        Analyzer(ticker, modifier = Modifier.fillMaxWidth().height(36.dp))
                        Row(Modifier.fillMaxWidth().padding(top = 2.dp)) {
                            Text("30Hz", style = MaterialTheme.typography.labelSmall, color = MuzikitColors.DarkGray)
                            Spacer(Modifier.weight(1f))
                            Text("FREQ//ANALYZER", style = MaterialTheme.typography.labelSmall, color = MuzikitColors.DarkGray)
                            Spacer(Modifier.weight(1f))
                            Text("16kHz", style = MaterialTheme.typography.labelSmall, color = MuzikitColors.DarkGray)
                        }
                    }
                } else {
                    Spacer(Modifier.weight(1f))
                }
            }
        }
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(
                p.title ?: "—",
                style = MaterialTheme.typography.bodyMedium,
                color = MuzikitColors.White,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f).clickable(enabled = hasMedia, onClick = onTitleClick)
                    .padding(start = 4.dp, top = 6.dp, bottom = 6.dp),
            )
            if (current != null) {
                val fav = current in m.favorites
                Box(
                    Modifier.size(36.dp).clickable { MusicHub.toggleFavorite(current) },
                    contentAlignment = Alignment.Center,
                ) {
                    GlyphIcon(if (fav) Glyph.Star else Glyph.StarOutline,
                        if (fav) MuzikitColors.White else MuzikitColors.Gray, 18.dp)
                }
            }
        }
        val dur = p.durationMs.coerceAtLeast(1)
        val shownMs = if (dragging) (dragValue * dur).toLong() else position
        Row(verticalAlignment = Alignment.CenterVertically) {
            TimeText(fmt(shownMs))
            Slider(
                value = if (dragging) dragValue else (position.toFloat() / dur).coerceIn(0f, 1f),
                onValueChange = { dragging = true; dragValue = it },
                onValueChangeFinished = {
                    val target = (dragValue * dur).toLong()
                    m.seekTo(target); position = target; dragging = false
                },
                enabled = hasMedia && p.durationMs > 0,
                colors = SliderDefaults.colors(
                    thumbColor = MuzikitColors.White,
                    activeTrackColor = MuzikitColors.White,
                    inactiveTrackColor = MuzikitColors.DarkGray,
                    disabledThumbColor = MuzikitColors.DarkGray,
                    disabledActiveTrackColor = MuzikitColors.DarkGray,
                    disabledInactiveTrackColor = MuzikitColors.DarkGray,
                ),
                modifier = Modifier.weight(1f).height(28.dp).padding(horizontal = 8.dp),
            )
            TimeText(fmt(p.durationMs))
        }
        Row(
            Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceEvenly,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            PanelButton(Glyph.Shuffle, enabled = true, active = m.play.shuffle, onClick = MusicHub::toggleShuffle)
            PanelButton(Glyph.Prev, enabled = hasMedia, onClick = MusicHub::prev)
            PanelButton(
                if (p.isPlaying) Glyph.Pause else Glyph.Play,
                enabled = p.connected && m.lib.songs.isNotEmpty(),
                onClick = MusicHub::togglePlay,
            )
            PanelButton(Glyph.Next, enabled = m.lib.songs.isNotEmpty(), onClick = { MusicHub.next(manual = true) })
            PanelButton(
                if (m.play.loop == LoopMode.ONE) Glyph.LoopOne else Glyph.Loop,
                enabled = true, active = m.play.loop != LoopMode.OFF, onClick = MusicHub::cycleLoop,
            )
            SleepButton(m.sleep)
        }
    }
}

@Composable
private fun TimeText(t: String) {
    Text(t, style = MaterialTheme.typography.labelSmall, color = MuzikitColors.Gray, maxLines = 1)
}

@Composable
private fun PanelButton(glyph: Glyph, enabled: Boolean, active: Boolean = true, onClick: () -> Unit) {
    Box(
        Modifier
            .size(48.dp)
            .clickable(enabled = enabled, onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        GlyphIcon(glyph, when {
            !enabled -> MuzikitColors.DarkGray
            active -> MuzikitColors.White
            else -> MuzikitColors.Gray.copy(alpha = 0.45f) // kapalı mod (karıştır/döngü)
        })
    }
}

/** Uyku düğmesi: "uyku" / "28dk" / "son". Kalan dakika sadece ekran görünürken güncellenir. */
@Composable
private fun SleepButton(sleep: SleepTimer) {
    var now by remember { mutableLongStateOf(android.os.SystemClock.elapsedRealtime()) }
    val lifecycle = LocalLifecycleOwner.current.lifecycle
    LaunchedEffect(sleep, lifecycle) {
        now = android.os.SystemClock.elapsedRealtime()
        if (sleep !is SleepTimer.At) return@LaunchedEffect
        lifecycle.repeatOnLifecycle(Lifecycle.State.STARTED) {
            while (true) {
                now = android.os.SystemClock.elapsedRealtime()
                delay(15_000)
            }
        }
    }
    val on = sleep != SleepTimer.Off
    Box(
        Modifier.size(48.dp).clickable(onClick = MusicHub::cycleSleep),
        contentAlignment = Alignment.Center,
    ) {
        Text(
            SleepTimer.shortLabel(sleep, now),
            style = MaterialTheme.typography.labelSmall,
            color = if (on) MuzikitColors.White else MuzikitColors.Gray.copy(alpha = 0.45f),
            maxLines = 1,
        )
    }
}

// ── Ayarlar ──────────────────────────────────────────────

@Composable
private fun SettingsTab(m: ScreenModel) {
    LazyColumn(Modifier.fillMaxSize().padding(horizontal = 16.dp)) {
        item { SectionTitle("çalma") }
        item {
            ToggleRow(
                "sadece kulaklık",
                "ses yalnızca kulaklığa gittiği doğrulanınca açılır; açıkken telefon hoparlörünün medya sesi kapatılır (tüm uygulamalar için; arama/zil/alarm etkilenmez), kapatınca geri gelir. bluetooth hoparlör izinsiz ayırt edilemez",
                m.settings.headphoneOnly, MusicHub::setHeadphoneOnly,
            )
        }
        item {
            ToggleRow(
                "bluetooth gelince devam et",
                "bluetooth kulaklık koptuysa ve aynı kulaklık 5 dk içinde geri gelirse kaldığı yerden sürer",
                m.settings.btResume, MusicHub::setBtResume,
            )
        }
        item {
            ToggleRow(
                "ses kısılınca duraklat",
                "medya sesi 0 olunca durur, ses açılınca devam eder",
                m.settings.mutePause, MusicHub::setMutePause,
            )
        }
        item {
            ToggleRow(
                "kulaklık tuşu: çift basış = sonraki",
                "tek tuşlu kulaklıklar için (çift → sonraki, üçlü → önceki). kulaklığın bunu kendisi yapıyorsa kapalı bırak; kapalıyken tek basış anında oynat/duraklat yapar",
                m.settings.headsetMultiTap, MusicHub::setHeadsetMultiTap,
            )
        }
        item {
            Row(
                Modifier.fillMaxWidth().clickable(onClick = MusicHub::cycleSleep).padding(vertical = 12.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Column(Modifier.weight(1f)) {
                    Text("uyku zamanlayıcısı", style = MaterialTheme.typography.bodyMedium, color = MuzikitColors.White)
                    Text("15 / 30 / 45 / 60 / 90 dk → şarkı sonu → kapalı; süre bitince duraklatır",
                        style = MaterialTheme.typography.bodySmall, color = MuzikitColors.Gray)
                }
                Spacer(Modifier.width(12.dp))
                Text(SleepTimer.longLabel(m.sleep, android.os.SystemClock.elapsedRealtime()),
                    style = MaterialTheme.typography.labelLarge, color = MuzikitColors.White, maxLines = 1)
            }
        }
        item { SectionTitle("görünüm") }
        item {
            ToggleRow(
                "frekans analizörü",
                "alttaki panelde çalan sese göre hareket eden çubuklar (gerçek ses verisi)",
                m.settings.analyzer, MusicHub::setAnalyzer,
            )
        }
        item {
            ToggleRow(
                "hareketli arka plan",
                "yavaş akan veri sütunları ve dönen radar; kapalıyken sadece sabit ızgara",
                m.settings.motion, MusicHub::setMotion,
            )
        }
        item { SectionTitle("kütüphane") }
        item {
            Row(Modifier.fillMaxWidth().padding(vertical = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                Text(
                    m.lib.folder?.lastPathSegment?.substringAfter(':') ?: "seçilmedi",
                    style = MaterialTheme.typography.bodyMedium, color = MuzikitColors.Gray,
                    maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f),
                )
                Spacer(Modifier.width(8.dp))
                SmallButton("yenile", MusicHub::reload)
                Spacer(Modifier.width(8.dp))
                SmallButton("değiştir", m.pickFolder)
            }
        }
        item { SyncBlock(m) }
        item { SectionTitle("kütüphane sağlık raporu") }
        item { ReportBlock(m.report, m.favorites.size) }
        item { Spacer(Modifier.height(24.dp)) }
    }
}

@Composable
private fun SectionTitle(text: String) {
    Text(
        text, style = MaterialTheme.typography.labelLarge, color = MuzikitColors.White,
        modifier = Modifier.padding(top = 18.dp, bottom = 4.dp),
    )
    Box(Modifier.fillMaxWidth().height(1.dp).background(MuzikitColors.DarkGray))
}

@Composable
private fun ToggleRow(title: String, desc: String, value: Boolean, onChange: (Boolean) -> Unit) {
    Row(
        Modifier.fillMaxWidth().clickable { onChange(!value) }.padding(vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f)) {
            Text(title, style = MaterialTheme.typography.bodyMedium, color = MuzikitColors.White)
            Text(desc, style = MaterialTheme.typography.bodySmall, color = MuzikitColors.Gray)
        }
        Spacer(Modifier.width(12.dp))
        // Sade anahtar: [ açık ] / [kapalı]
        Text(
            if (value) "açık" else "kapalı",
            style = MaterialTheme.typography.labelLarge,
            color = if (value) MuzikitColors.Black else MuzikitColors.Gray,
            maxLines = 1,
            modifier = Modifier
                .border(1.dp, if (value) MuzikitColors.White else MuzikitColors.DarkGray)
                .background(if (value) MuzikitColors.White else MuzikitColors.Black)
                .padding(horizontal = 10.dp, vertical = 5.dp),
        )
    }
}

@Composable
private fun ReportBlock(r: LibraryReport?, favorites: Int) {
    if (r == null) {
        Text("kütüphane okunamadı", style = MaterialTheme.typography.bodySmall, color = MuzikitColors.Gray,
            modifier = Modifier.padding(vertical = 8.dp))
        return
    }
    Column(Modifier.padding(vertical = 8.dp)) {
        val rows = listOf(
            "opus dosyası" to r.total.toString(),
            "en yüksek numara" to r.maxNumber.toString().padStart(4, '0'),
            "numarasız dosya" to r.unnumbered.toString(),
            "downloaded.txt kaydı" to (r.downloadedIds?.toString() ?: "dosya yok"),
            "favori" to favorites.toString(),
        )
        rows.forEach { (k, v) ->
            Row(Modifier.fillMaxWidth().padding(vertical = 2.dp)) {
                Text(k, style = MaterialTheme.typography.bodySmall, color = MuzikitColors.Gray, modifier = Modifier.weight(1f))
                Text(v, style = MaterialTheme.typography.bodySmall, color = MuzikitColors.White)
            }
        }
        Spacer(Modifier.height(8.dp))
        if (r.missing.isEmpty()) {
            Text("✓ numaralandırmada eksik yok", style = MaterialTheme.typography.bodySmall, color = MuzikitColors.Gray)
        } else {
            Text("! ${r.missing.size} eksik numara:", style = MaterialTheme.typography.bodySmall, color = MuzikitColors.White)
            Text(r.missing.take(500).joinToString(", ") { it.toString().padStart(4, '0') },
                style = MaterialTheme.typography.bodySmall, color = MuzikitColors.Gray)
        }
        if (r.duplicates.isNotEmpty()) {
            Spacer(Modifier.height(6.dp))
            Text("! aynı numaralı dosyalar:", style = MaterialTheme.typography.bodySmall, color = MuzikitColors.White)
            Text(r.duplicates.entries.joinToString(", ") { (n, c) -> "${n.toString().padStart(4, '0')} ×$c" },
                style = MaterialTheme.typography.bodySmall, color = MuzikitColors.Gray)
        }
    }
}

// ── Bilgisayarla eşitleme ────────────────────────────────

@Composable
private fun SyncBlock(m: ScreenModel) {
    val ctx = androidx.compose.ui.platform.LocalContext.current
    val s = m.sync
    // Eşitleme açıkken ekran kapanmasın (aktarım yarıda kalmasın)
    val view = androidx.compose.ui.platform.LocalView.current
    androidx.compose.runtime.DisposableEffect(s.running) {
        view.keepScreenOn = s.running
        onDispose { view.keepScreenOn = false }
    }
    var askReset by remember { mutableStateOf(false) }
    if (askReset) {
        androidx.compose.material3.AlertDialog(
            onDismissRequest = { askReset = false },
            containerColor = MuzikitColors.Panel,
            title = { Text("eşitleme geçmişi silinsin mi?", color = MuzikitColors.White) },
            text = {
                Text(
                    "tüm eşleşmeler, anahtarlar ve doğrulama kayıtları silinir; bir sonraki eşitlemede bilgisayara yeniden kod girmen gerekir. şarkılara ve indirme geçmişine dokunulmaz.",
                    color = MuzikitColors.Gray, style = MaterialTheme.typography.bodySmall,
                )
            },
            confirmButton = { SmallButton("sil", { com.muzikit.app.sync.SyncServer.resetAll(ctx); askReset = false }) },
            dismissButton = { SmallButton("vazgeç", { askReset = false }, active = false) },
        )
    }
    Column(Modifier.fillMaxWidth()) {
        SectionTitle("bilgisayarla eşitle")
        Text(
            "aynı wi-fi ağında; sadece farklı şarkılar karşıya gider, var olan numaralar değişmez. bilgisayarda: ayarlar & kısayollar → eşitle",
            style = MaterialTheme.typography.bodySmall, color = MuzikitColors.Gray, modifier = Modifier.padding(vertical = 6.dp),
        )
        Row(verticalAlignment = Alignment.CenterVertically) {
            if (!s.running) SmallButton("eşitlemeyi aç", { com.muzikit.app.sync.SyncServer.start(ctx) })
            else SmallButton("kapat", { com.muzikit.app.sync.SyncServer.stop() })
            Spacer(Modifier.width(8.dp))
            if (!s.running) SmallButton("eşitleme geçmişini sil", { askReset = true }, active = false)
        }
        if (s.running) {
            Spacer(Modifier.height(8.dp))
            s.code?.let {
                Text("eşleştirme kodu", style = MaterialTheme.typography.bodySmall, color = MuzikitColors.Gray)
                Text(it, style = MaterialTheme.typography.headlineMedium, color = MuzikitColors.White)
            }
            Text("adres: ${s.address ?: "wi-fi bulunamadı"}", style = MaterialTheme.typography.bodySmall, color = MuzikitColors.Gray)
            Text(s.status, style = MaterialTheme.typography.bodyMedium, color = MuzikitColors.White, modifier = Modifier.padding(top = 4.dp))
        }
        s.log.takeLast(8).forEach {
            Text(it, style = MaterialTheme.typography.labelSmall, color = MuzikitColors.Gray, maxLines = 2, overflow = TextOverflow.Ellipsis)
        }
    }
}
