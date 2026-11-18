package com.muzikit.app.download

import com.muzikit.app.download.UrlRules.Normalized
import com.muzikit.app.download.UrlRules.extractVideoId
import com.muzikit.app.download.UrlRules.isValidBitrate
import com.muzikit.app.download.UrlRules.isValidVideoId
import com.muzikit.app.download.UrlRules.normalizeUrl
import com.muzikit.app.download.UrlRules.sanitizeFilename
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** Masaüstü `src/utils.rs` testlerinin birebir karşılığı + Android'e özgü ekler. */
class UrlRulesTest {

    @Test
    fun test_extract_video_id() {
        assertEquals("dQw4w9WgXcQ", extractVideoId("https://www.youtube.com/watch?v=dQw4w9WgXcQ"))
        assertEquals("dQw4w9WgXcQ", extractVideoId("https://youtu.be/dQw4w9WgXcQ?t=10"))
        assertEquals("dQw4w9WgXcQ", extractVideoId("youtube.com/watch?v=dQw4w9WgXcQ&list=RDdQw4w9WgXcQ"))
    }

    @Test
    fun test_sanitize_filename() {
        assertEquals("Şarkı Adı Test Grup", sanitizeFilename("Şarkı Adı: / Test ? * <Grup>", 100))
    }

    @Test
    fun test_sanitize_filename_blocks_traversal_and_escapes() {
        assertEquals("etcpasswd", sanitizeFilename("../../etc/passwd", 100))
        assertEquals("Bilinmeyen", sanitizeFilename("..", 100))
        assertEquals("Bilinmeyen", sanitizeFilename("", 100))
        assertEquals("a[31mred[0m b", sanitizeFilename("a\u001b[31mred\u001b[0m\nb", 100))
        assertEquals("abcgpj.exe", sanitizeFilename("abc‮gpj.exe", 100))
    }

    @Test
    fun test_url_allowlist_and_injection() {
        // Seçenek enjeksiyonu (yt-dlp --exec) ve yabancı alan adları reddedilir
        assertNull(normalizeUrl("--exec=rm -rf ~"))
        assertNull(normalizeUrl("-o/tmp/x"))
        assertNull(normalizeUrl("https://evil.com/watch?v=dQw4w9WgXcQ"))
        assertNull(normalizeUrl("https://youtube.com.evil.com/watch?v=dQw4w9WgXcQ"))
        assertNull(normalizeUrl("https://youtube.com@evil.com/watch?v=dQw4w9WgXcQ"))
        assertNull(normalizeUrl("file:///etc/passwd"))
        assertNull(normalizeUrl("https://www.youtube.com/watch?v=dQw4w9WgXcQ\n--exec"))
        // Unicode harfli sahte ID kabul edilmez
        assertNull(extractVideoId("https://youtu.be/şşşşşşşşşşş"))
        // Geçerli girdiler sabit, güvenli biçime çevrilir
        assertEquals(
            Normalized("https://www.youtube.com/playlist?list=RDus8FZU5H7so", true),
            normalizeUrl("https://www.youtube.com/watch?v=us8FZU5H7so&list=RDus8FZU5H7so&t=986s"),
        )
        assertEquals(
            Normalized("https://www.youtube.com/watch?v=dQw4w9WgXcQ", false),
            normalizeUrl("https://music.youtube.com/watch?v=dQw4w9WgXcQ"),
        )
        // Geçersiz karakterli playlist ID'si playlist sayılmaz
        assertEquals(
            Normalized("https://www.youtube.com/watch?v=dQw4w9WgXcQ", false),
            normalizeUrl("https://www.youtube.com/watch?v=dQw4w9WgXcQ&list=ab;rm"),
        )
    }

    @Test
    fun test_video_id_and_bitrate_validation() {
        assertTrue(isValidVideoId("dQw4w9WgXcQ"))
        assertFalse(isValidVideoId("dQw4w9WgXc"))
        assertFalse(isValidVideoId("dQw4w9Wg\nXc"))
        assertFalse(isValidVideoId("--exec=abcd"))
        assertTrue(isValidBitrate("128k"))
        assertFalse(isValidBitrate("128k -y"))
        assertFalse(isValidBitrate("9999k"))
        assertFalse(isValidBitrate("-filter_complex"))
    }

    // ── Android'e özgü ek kontroller ─────────────────────

    @Test
    fun moreHostileInputs() {
        assertNull(normalizeUrl("https://youtube.com:8080/watch?v=dQw4w9WgXcQ"))       // port
        assertNull(normalizeUrl("javascript:alert(1)//youtube.com/watch?v=dQw4w9WgXcQ"))
        assertNull(normalizeUrl("https://www.youtube.com/watch?v=dQw4w9WgXcQ\u0000"))
        assertNull(normalizeUrl("https://www.youtube.com/watch?v=dQw4w9WgXcQ x")) // görünmez boşluk
        assertNull(normalizeUrl("https://www.youtube.com/" + "a".repeat(3000)))
        assertNull(normalizeUrl("https://www.youtube.com/watch?v=dQw4w9WgXc"))         // 10 karakter
        assertEquals(
            Normalized("https://www.youtube.com/watch?v=dQw4w9WgXcQ", false),
            normalizeUrl("  HTTPS://M.YOUTUBE.COM/watch?v=dQw4w9WgXcQ  "),
        )
        assertEquals("dQw4w9WgXcQ", extractVideoId("https://www.youtube.com/shorts/dQw4w9WgXcQ"))
    }

    @Test
    fun uniqueNameNeverOverwrites() {
        val taken = mutableSetOf("0806 - Ad.opus")
        assertEquals("0806 - Ad [dQw4w9WgXcQ].opus", UrlRules.uniqueName({ it in taken }, 806, "Ad", "dQw4w9WgXcQ"))
        taken += "0806 - Ad [dQw4w9WgXcQ].opus"
        assertEquals("0806 - Ad [dQw4w9WgXcQ] (2).opus", UrlRules.uniqueName({ it in taken }, 806, "Ad", "dQw4w9WgXcQ"))
        assertEquals("0001 - X.opus", UrlRules.uniqueName({ false }, 1, "X", "dQw4w9WgXcQ"))
    }

    @Test
    fun nextNumberAndDownloadedIds() {
        assertEquals(805, UrlRules.nextNumber(listOf("0001 - a.opus", "0804 - b.opus", "downloaded.txt", "9999.mp3")))
        assertEquals(1, UrlRules.nextNumber(emptyList()))
        val dl = UrlRules.parseDownloaded("dQw4w9WgXcQ\n  us8FZU5H7so \nbozuk\n")
        assertEquals(setOf("dQw4w9WgXcQ", "us8FZU5H7so"), dl)
        assertEquals(
            listOf("abcdefghijk"),
            UrlRules.newIds(listOf("dQw4w9WgXcQ", "abcdefghijk", "abcdefghijk", "--exec=abcd"), dl),
        )
    }

    @Test
    fun splitLinksAndPermanentErrors() {
        assertEquals(listOf("a", "b", "c"), UrlRules.splitLinks(" a\nb   c "))
        assertTrue(UrlRules.isPermanentError("ERROR: Sign in to confirm your age"))
        assertTrue(UrlRules.isPermanentError("ERROR: Private video"))
        assertFalse("'page' yaş hatası sayılmaz", UrlRules.isPermanentError("ERROR: unable to download webpage"))
    }

    @Test
    fun longTitleCutAtWordBoundary() {
        assertEquals("aaa bbb", sanitizeFilename("aaa bbb ccc", 9))
        assertEquals("ççççç", sanitizeFilename("çççççççç", 5))
    }
}
