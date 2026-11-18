package com.muzikit.app.sync

import java.io.InputStream
import java.io.OutputStream
import javax.crypto.Cipher
import javax.crypto.spec.GCMParameterSpec
import javax.crypto.spec.SecretKeySpec

/**
 * AES-256-GCM akış şifrelemesi (protokol 2). Gövde = önek(8) ‖ parça₀ ‖ parça₁ ‖ …;
 * her parça ≤ 65536 bayt düz metin + 16 bayt etiket; nonce = önek ‖ sıra(4, BE); aad = bağlam|sıra|son.
 * Bellek: bir parça (64 KB) — 100 MB dosya belleğe alınmaz.
 */
object SyncCipher {
    const val CHUNK = 65536
    private const val TAG = 16
    const val HEADER_VALUE = "aes256gcm-stream-v1"

    class DecryptError(msg: String) : Exception(msg)

    fun encKey(k: ByteArray): ByteArray = SyncCrypto.hkdf(k, ByteArray(32), "muzikit-sync-v1|enc".toByteArray(), 32)

    fun chunks(plainLen: Long): Long = maxOf(1L, (plainLen + CHUNK - 1) / CHUNK)
    fun cipherLength(plainLen: Long): Long = 8 + plainLen + TAG * chunks(plainLen)

    fun requestContext(method: String, path: String, time: Long, nonce: String) =
        listOf("req", method, path, time.toString(), nonce).joinToString("\n")

    fun responseContext(status: Int, path: String, time: Long, requestNonce: String) =
        listOf("resp", status.toString(), path, time.toString(), requestNonce).joinToString("\n")

    private fun cipher(mode: Int, e: ByteArray, prefix: ByteArray, i: Long, ctx: String, last: Boolean): Cipher =
        Cipher.getInstance("AES/GCM/NoPadding").apply {
            val nonce = prefix + byteArrayOf((i shr 24).toByte(), (i shr 16).toByte(), (i shr 8).toByte(), i.toByte())
            init(mode, SecretKeySpec(e, "AES"), GCMParameterSpec(TAG * 8, nonce))
            updateAAD("$ctx|$i|${if (last) 1 else 0}".toByteArray(Charsets.UTF_8))
        }

    /** [plainLen] baytlık düz akışı şifreleyip [out]'a yazar. [prefix] testte sabitlenebilir. */
    fun encrypt(
        e: ByteArray, ctx: String, input: InputStream, plainLen: Long, out: OutputStream,
        prefix: ByteArray = SyncCrypto.randomBytes(8),
    ) {
        require(prefix.size == 8)
        out.write(prefix)
        val n = chunks(plainLen)
        val buf = ByteArray(CHUNK)
        var left = plainLen
        for (i in 0 until n) {
            val len = minOf(CHUNK.toLong(), left).toInt()
            readFully(input, buf, len)
            left -= len
            out.write(cipher(Cipher.ENCRYPT_MODE, e, prefix, i, ctx, i == n - 1).doFinal(buf, 0, len))
        }
    }

    fun encryptBytes(e: ByteArray, ctx: String, plain: ByteArray, prefix: ByteArray = SyncCrypto.randomBytes(8)): ByteArray =
        java.io.ByteArrayOutputStream().also { encrypt(e, ctx, plain.inputStream(), plain.size.toLong(), it, prefix) }.toByteArray()

    /** [cipherLen] baytlık şifreli akışı çözüp [out]'a yazar; döner: düz bayt sayısı. Bozuk/kesik → [DecryptError]. */
    fun decrypt(e: ByteArray, ctx: String, input: InputStream, cipherLen: Long, out: OutputStream): Long {
        if (cipherLen < 8 + TAG) throw DecryptError("short")
        val prefix = ByteArray(8).also { readFully(input, it, 8) }
        var left = cipherLen - 8
        var i = 0L
        var plain = 0L
        val buf = ByteArray(CHUNK + TAG)
        while (true) {
            val last = left <= CHUNK + TAG
            val len = if (last) left.toInt() else CHUNK + TAG
            if (len < TAG) throw DecryptError("short")
            readFully(input, buf, len)
            val p = try {
                cipher(Cipher.DECRYPT_MODE, e, prefix, i, ctx, last).doFinal(buf, 0, len)
            } catch (x: Exception) { throw DecryptError("auth") }
            out.write(p); plain += p.size; left -= len; i++
            if (last) return plain
        }
    }

    fun decryptBytes(e: ByteArray, ctx: String, body: ByteArray): ByteArray =
        java.io.ByteArrayOutputStream().also { decrypt(e, ctx, body.inputStream(), body.size.toLong(), it) }.toByteArray()

    private fun readFully(input: InputStream, b: ByteArray, len: Int) {
        var off = 0
        while (off < len) { val n = input.read(b, off, len - off); if (n < 0) throw DecryptError("eof"); off += n }
    }
}
