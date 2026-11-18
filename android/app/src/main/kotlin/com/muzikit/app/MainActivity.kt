package com.muzikit.app

import android.os.Bundle
import android.provider.DocumentsContract
import android.util.Log
import androidx.activity.ComponentActivity
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.runtime.getValue
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.muzikit.app.playback.PlayerConnection
import com.muzikit.app.ui.KeyAction
import com.muzikit.app.ui.MainScreen
import com.muzikit.app.ui.keyAction
import com.muzikit.app.ui.ScreenModel
import com.muzikit.app.ui.theme.MuzikitTheme

const val TAG = "muzikit"

/** Klasör seçicide önerilen başlangıç: telefondaki `Music/Müzik`. */
private val SUGGESTED_FOLDER = DocumentsContract.buildDocumentUri(
    "com.android.externalstorage.documents", "primary:Music/Müzik"
)

class MainActivity : ComponentActivity() {

    private lateinit var player: PlayerConnection

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        MusicHub.init(applicationContext)
        player = PlayerConnection(applicationContext)
        Log.i(TAG, "MainActivity açıldı")

        setContent {
            MuzikitTheme {
                // Akışlar sadece ekran görünürken toplanır (arka planda arayüz hiçbir iş yapmaz).
                val lib by MusicHub.library.collectAsStateWithLifecycle()
                val visible by MusicHub.visible.collectAsStateWithLifecycle()
                val view by MusicHub.view.collectAsStateWithLifecycle()
                val favorites by MusicHub.favorites.collectAsStateWithLifecycle()
                val play by MusicHub.play.collectAsStateWithLifecycle()
                val toast by MusicHub.toast.collectAsStateWithLifecycle()
                val ui by player.ui.collectAsStateWithLifecycle()
                val settings by MusicHub.settings.collectAsStateWithLifecycle()
                val sleep by MusicHub.sleep.collectAsStateWithLifecycle()
                val report by MusicHub.report.collectAsStateWithLifecycle()
                val download by com.muzikit.app.download.DownloadCenter.state.collectAsStateWithLifecycle()
                val sync by com.muzikit.app.sync.SyncServer.state.collectAsStateWithLifecycle()
                val picker = rememberLauncherForActivityResult(
                    ActivityResultContracts.OpenDocumentTree()
                ) { uri -> if (uri != null) MusicHub.onFolderPicked(uri) }

                MainScreen(
                    ScreenModel(
                        lib = lib, visible = visible, view = view, favorites = favorites,
                        play = play, player = ui, toast = toast,
                        settings = settings, sleep = sleep, report = report, download = download, sync = sync,
                        position = player::positionMs,
                        seekTo = player::seekTo,
                        pickFolder = { picker.launch(lib.folder ?: SUGGESTED_FOLDER) },
                    )
                )
            }
        }
    }

    /**
     * Harici klavye kısayolları: ekrandaki düğmeler tuşu almadan ÖNCE yakalanır (yoksa boşluk odaktaki düğmeye
     * "tıklar", oklar odağı gezdirir). Arama kutusuna yazarken kısayollar devre dışı.
     */
    override fun dispatchKeyEvent(event: android.view.KeyEvent): Boolean {
        if (!MusicHub.textInputFocused && event.action == android.view.KeyEvent.ACTION_DOWN &&
            !event.isCtrlPressed && !event.isAltPressed && handleShortcut(event)
        ) return true
        // Kısayolun bırakma (UP) olayını da yut ki odaktaki düğme tetiklenmesin.
        if (!MusicHub.textInputFocused && event.action == android.view.KeyEvent.ACTION_UP &&
            keyAction(event.keyCode, charOf(event), event.isShiftPressed) != null
        ) return true
        return super.dispatchKeyEvent(event)
    }

    private fun charOf(e: android.view.KeyEvent): Char = e.unicodeChar.takeIf { it != 0 }?.toChar() ?: '\u0000'

    private fun handleShortcut(event: android.view.KeyEvent): Boolean {
        val action = keyAction(event.keyCode, charOf(event), event.isShiftPressed) ?: return false
        when (action) {
            KeyAction.PLAY_PAUSE -> MusicHub.togglePlay()
            KeyAction.NEXT -> MusicHub.next(manual = true)
            KeyAction.PREV -> MusicHub.prev()
            KeyAction.SEEK_BACK -> MusicHub.seekBy(-5_000)
            KeyAction.SEEK_FWD -> MusicHub.seekBy(5_000)
            KeyAction.SEEK_BACK_LONG -> MusicHub.seekBy(-30_000)
            KeyAction.SEEK_FWD_LONG -> MusicHub.seekBy(30_000)
            KeyAction.VOL_UP -> MusicHub.adjustVolume(android.media.AudioManager.ADJUST_RAISE)
            KeyAction.VOL_DOWN -> MusicHub.adjustVolume(android.media.AudioManager.ADJUST_LOWER)
            KeyAction.MUTE -> MusicHub.adjustVolume(android.media.AudioManager.ADJUST_TOGGLE_MUTE)
            KeyAction.SHUFFLE -> MusicHub.toggleShuffle()
            KeyAction.LOOP -> MusicHub.cycleLoop()
            KeyAction.SLEEP -> MusicHub.cycleSleep()
            KeyAction.SEARCH -> MusicHub.requestUi(UiEvent.FOCUS_SEARCH)
            KeyAction.FAVORITE -> MusicHub.toggleFavoriteCurrent()
            KeyAction.ONLY_FAVORITES -> MusicHub.toggleOnlyFavorites()
            KeyAction.REVEAL_PLAYING -> MusicHub.requestUi(UiEvent.REVEAL_PLAYING)
            KeyAction.REFRESH -> MusicHub.reload()
        }
        return true
    }

    // Arayüz yalnızca görünürken servise bağlı; arka planda sadece PlaybackService çalışır.
    override fun onStart() {
        super.onStart()
        MusicHub.uiVisible = true
        player.connect()
    }

    override fun onStop() {
        MusicHub.uiVisible = false
        MusicHub.savePosition()
        player.release()
        super.onStop()
    }
}
