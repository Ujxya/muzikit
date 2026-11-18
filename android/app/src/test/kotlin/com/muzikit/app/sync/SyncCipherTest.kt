package com.muzikit.app.sync

import org.json.JSONObject
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.fail
import org.junit.Test
import java.io.File

class SyncCipherTest {

    private val v: JSONObject by lazy {
        val f = listOf(File("../../sync-test-vectors.json"), File("../sync-test-vectors.json")).firstOrNull { it.isFile }
        org.junit.Assume.assumeTrue("sync-test-vectors.json yok, atlandı", f != null)
        JSONObject(f!!.readText())
    }
    private val k get() = SyncCrypto.unhex(v.getJSONObject("crypto").getString("key"))
    private val enc get() = v.getJSONObject("encryption")

    @Test
    fun sharedVector() {
        val e = SyncCipher.encKey(k)
        assertEquals(enc.getString("encKey"), SyncCrypto.hex(e))
        val ctx = enc.getString("context"); val prefix = SyncCrypto.unhex(enc.getString("prefix"))
        val small = enc.getJSONObject("small")
        assertEquals(small.getString("cipher"), SyncCrypto.hex(SyncCipher.encryptBytes(e, ctx, small.getString("plain").toByteArray(), prefix)))
        assertEquals(enc.getJSONObject("empty").getString("cipher"), SyncCrypto.hex(SyncCipher.encryptBytes(e, ctx, ByteArray(0), prefix)))
        val big = ByteArray(65546) { ((it * 7) % 251).toByte() }
        val two = enc.getJSONObject("twoChunks")
        assertEquals(two.getString("sha256Plain"), SyncCrypto.sha256Hex(big))
        val c = SyncCipher.encryptBytes(e, ctx, big, prefix)
        assertEquals(two.getInt("cipherLength"), c.size)
        assertEquals(two.getString("sha256Cipher"), SyncCrypto.sha256Hex(c))
        assertArrayEquals(big, SyncCipher.decryptBytes(e, ctx, c))
    }

    @Test
    fun roundTripSizes() {
        val e = SyncCipher.encKey(ByteArray(32) { 1 })
        for (n in listOf(0, 1, 65535, 65536, 65537, 131072, 200_000)) {
            val p = ByteArray(n) { (it % 256).toByte() }
            val c = SyncCipher.encryptBytes(e, "ctx", p)
            assertEquals(SyncCipher.cipherLength(n.toLong()), c.size.toLong())
            assertArrayEquals(p, SyncCipher.decryptBytes(e, "ctx", c))
        }
    }

    private fun mustFail(block: () -> Unit) {
        try { block(); fail("çözme başarısız olmalıydı") } catch (_: SyncCipher.DecryptError) {}
    }

    @Test
    fun tamperTruncateWrongContextFail() {
        val e = SyncCipher.encKey(ByteArray(32) { 2 })
        val p = ByteArray(140_000) { it.toByte() }
        val c = SyncCipher.encryptBytes(e, "ctx", p)
        mustFail { SyncCipher.decryptBytes(e, "ctx", c.copyOf().also { it[100] = (it[100] + 1).toByte() }) } // bit değişti
        mustFail { SyncCipher.decryptBytes(e, "ctx", c.copyOf(8 + 2 * (65536 + 16))) } // son parça kesildi
        mustFail { SyncCipher.decryptBytes(e, "baska", c) } // başka isteğe/yanıta taşınmış
        mustFail { SyncCipher.decryptBytes(SyncCipher.encKey(ByteArray(32) { 3 }), "ctx", c) } // yanlış anahtar
    }
}
