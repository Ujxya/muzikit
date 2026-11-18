package com.muzikit.app.ui

import android.view.KeyEvent
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class KeyShortcutsTest {

    @Test
    fun desktopDefaults() {
        assertEquals(KeyAction.PLAY_PAUSE, keyAction(KeyEvent.KEYCODE_SPACE, ' ', false))
        assertEquals(KeyAction.PLAY_PAUSE, keyAction(KeyEvent.KEYCODE_P, 'p', false))
        assertEquals(KeyAction.NEXT, keyAction(KeyEvent.KEYCODE_N, 'n', false))
        assertEquals(KeyAction.PREV, keyAction(KeyEvent.KEYCODE_B, 'b', false))
        assertEquals(KeyAction.SEEK_BACK, keyAction(KeyEvent.KEYCODE_DPAD_LEFT, '\u0000', false))
        assertEquals(KeyAction.SEEK_FWD_LONG, keyAction(KeyEvent.KEYCODE_DPAD_RIGHT, '\u0000', true))
        assertEquals(KeyAction.VOL_UP, keyAction(KeyEvent.KEYCODE_EQUALS, '+', true))
        assertEquals(KeyAction.LOOP, keyAction(KeyEvent.KEYCODE_L, 'l', false))
        assertEquals(KeyAction.SEARCH, keyAction(KeyEvent.KEYCODE_SLASH, '/', false))
    }

    @Test
    fun shiftedLetterIsDifferentAction() {
        assertEquals(KeyAction.FAVORITE, keyAction(KeyEvent.KEYCODE_F, 'f', false))
        assertEquals(KeyAction.ONLY_FAVORITES, keyAction(KeyEvent.KEYCODE_F, 'F', true))
    }

    @Test
    fun unknownKeyIgnored() {
        assertNull(keyAction(KeyEvent.KEYCODE_Z, 'z', false))
        assertNull(keyAction(KeyEvent.KEYCODE_ENTER, '\n', false))
    }
}
