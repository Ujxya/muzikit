package com.muzikit.app.ui

import android.view.KeyEvent

/** Harici klavye kısayolları — masaüstü `src/keymap.rs` varsayılanlarıyla aynı tuşlar. */
enum class KeyAction {
    PLAY_PAUSE, NEXT, PREV,
    SEEK_BACK, SEEK_FWD, SEEK_BACK_LONG, SEEK_FWD_LONG,
    VOL_UP, VOL_DOWN, MUTE,
    SHUFFLE, LOOP, SLEEP,
    SEARCH, FAVORITE, ONLY_FAVORITES, REVEAL_PLAYING, REFRESH,
}

/**
 * Tuş → eylem. Yazı kutusu odaktayken çağrılmaz (Activity.onKeyDown'a sadece tüketilmemiş tuşlar gelir).
 * [char]: tuşun ürettiği karakter (klavye düzenine göre; "+" "/" "F" gibi), yoksa 0.
 */
fun keyAction(keyCode: Int, char: Char, shift: Boolean): KeyAction? {
    when (keyCode) {
        KeyEvent.KEYCODE_SPACE -> return KeyAction.PLAY_PAUSE
        KeyEvent.KEYCODE_DPAD_LEFT -> return if (shift) KeyAction.SEEK_BACK_LONG else KeyAction.SEEK_BACK
        KeyEvent.KEYCODE_DPAD_RIGHT -> return if (shift) KeyAction.SEEK_FWD_LONG else KeyAction.SEEK_FWD
    }
    return when (char) {
        'p' -> KeyAction.PLAY_PAUSE
        'n' -> KeyAction.NEXT
        'b' -> KeyAction.PREV
        '+', '=' -> KeyAction.VOL_UP
        '-' -> KeyAction.VOL_DOWN
        'm' -> KeyAction.MUTE
        's' -> KeyAction.SHUFFLE
        'd', 'l' -> KeyAction.LOOP
        't' -> KeyAction.SLEEP
        '/' -> KeyAction.SEARCH
        'f' -> KeyAction.FAVORITE
        'F' -> KeyAction.ONLY_FAVORITES
        'c' -> KeyAction.REVEAL_PLAYING
        'r' -> KeyAction.REFRESH
        else -> null
    }
}
