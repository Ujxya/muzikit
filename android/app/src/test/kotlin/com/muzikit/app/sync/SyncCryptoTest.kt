package com.muzikit.app.sync

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class SyncCryptoTest {

    @Test
    fun hkdfRfc5869TestCase1() {
        val ikm = SyncCrypto.unhex("0b".repeat(22))
        val salt = SyncCrypto.unhex("000102030405060708090a0b0c")
        val info = SyncCrypto.unhex("f0f1f2f3f4f5f6f7f8f9")
        assertEquals(
            "3cb25f25faacd57a90434f64d0362f2a2d2d0a90cf1a5a4c5db02d56ecc4c5bf34007208d5b887185865",
            SyncCrypto.hex(SyncCrypto.hkdf(ikm, salt, info, 42)),
        )
    }

    @Test
    fun hmacRfc4231TestCase2() {
        assertEquals(
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843",
            SyncCrypto.hex(SyncCrypto.hmac("Jefe".toByteArray(), "what do ya want for nothing?".toByteArray())),
        )
    }

    @Test
    fun codeFormatAndNormalize() {
        val c = SyncCrypto.newCode()
        assertEquals(12, c.length)
        assertTrue(c.all { it in 'A'..'Z' || it in '2'..'7' })
        assertEquals(c, SyncCrypto.normalize(SyncCrypto.display(c).lowercase().replace("-", " - ")))
    }

    @Test
    fun keyDependsOnAllInputs() {
        val salt = ByteArray(16) { it.toByte() }
        val k = SyncCrypto.deriveKey("ABCD-EFGH-2345", salt, "phone1", "pc1")
        assertEquals(32, k.size)
        assertEquals(SyncCrypto.hex(k), SyncCrypto.hex(SyncCrypto.deriveKey("abcdefgh2345", salt, "phone1", "pc1")))
        assertNotEquals(SyncCrypto.hex(k), SyncCrypto.hex(SyncCrypto.deriveKey("ABCDEFGH2346", salt, "phone1", "pc1")))
        assertNotEquals(SyncCrypto.hex(k), SyncCrypto.hex(SyncCrypto.deriveKey("ABCDEFGH2345", salt, "phone1", "pc2")))
    }

    @Test
    fun signVerifyAndTamper() {
        val key = ByteArray(32) { 7 }
        val t = SyncCrypto.requestText("GET", "/v1/index", 1_800_000_000, "a".repeat(32), SyncCrypto.sha256Hex(ByteArray(0)))
        val sig = SyncCrypto.sign(key, t)
        assertTrue(SyncCrypto.verify(key, t, sig))
        assertFalse(SyncCrypto.verify(key, t.replace("/v1/index", "/v1/indey"), sig))
        assertEquals("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855", SyncCrypto.sha256Hex(ByteArray(0)))
        assertEquals("resp\n200\n/v1/hello\n5\nn\ns", SyncCrypto.responseText(200, "/v1/hello", 5, "n", "s"))
    }

    @Test
    fun replayGuard() {
        val g = SyncCrypto.ReplayGuard()
        val n = "0123456789abcdef0123456789abcdef"
        assertTrue(g.accept(n, 1000, 1000))
        assertFalse("tekrar", g.accept(n, 1000, 1010))
        assertFalse("zaman dışı", g.accept("f".repeat(32), 1000, 1061))
        assertFalse("bozuk nonce", g.accept("xyz", 1000, 1000))
        assertTrue("120 sn sonra unutulur", g.accept(n, 1200, 1200))
    }
}

/** Ortak dosyadaki şifreleme örneği: masaüstü (Rust) aynı değerleri üretmeli. */
class SyncCryptoSharedVectorTest {
    @org.junit.Test
    fun matchesSharedVector() {
        val f = listOf(java.io.File("../../sync-test-vectors.json"), java.io.File("../sync-test-vectors.json")).firstOrNull { it.isFile }
        org.junit.Assume.assumeTrue("sync-test-vectors.json yok, atlandı", f != null)
        val c = org.json.JSONObject(f!!.readText()).getJSONObject("crypto")
        val key = SyncCrypto.deriveKey(c.getString("code"), SyncCrypto.unhex(c.getString("salt")), c.getString("phoneId"), c.getString("pcId"))
        org.junit.Assert.assertEquals(c.getString("key"), SyncCrypto.hex(key))
        val rq = c.getJSONObject("request"); val rs = c.getJSONObject("response")
        org.junit.Assert.assertEquals(rq.getString("text"), SyncCrypto.requestText("GET", "/v1/index", 1800000000, "00112233445566778899aabbccddeeff", SyncCrypto.sha256Hex(ByteArray(0))))
        org.junit.Assert.assertEquals(rq.getString("auth"), SyncCrypto.sign(key, rq.getString("text")))
        org.junit.Assert.assertEquals(rs.getString("text"), SyncCrypto.responseText(200, "/v1/index", 1800000001, "00112233445566778899aabbccddeeff", SyncCrypto.sha256Hex(rs.getString("body").toByteArray())))
        org.junit.Assert.assertEquals(rs.getString("auth"), SyncCrypto.sign(key, rs.getString("text")))
    }
}
