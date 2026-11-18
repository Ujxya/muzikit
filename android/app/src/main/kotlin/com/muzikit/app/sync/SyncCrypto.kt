package com.muzikit.app.sync

import java.security.MessageDigest
import java.security.SecureRandom
import javax.crypto.Mac
import javax.crypto.spec.SecretKeySpec

/** Eşitleme güvenliği: eşleştirme kodu, HKDF, HMAC imzaları, nonce/zaman denetimi (Android'den bağımsız, testli). */
object SyncCrypto {
    private const val B32 = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567"
    private val rnd = SecureRandom()

    /** 12 karakter base32 (≈60 bit). Ekranda `XXXX-XXXX-XXXX`. */
    fun newCode(): String = buildString { repeat(12) { append(B32[rnd.nextInt(32)]) } }
    fun display(code: String) = code.chunked(4).joinToString("-")
    fun normalize(input: String): String = input.replace("-", "").replace(" ", "").uppercase()

    fun randomBytes(n: Int) = ByteArray(n).also { rnd.nextBytes(it) }
    fun randomHex(nBytes: Int) = hex(randomBytes(nBytes))

    fun hex(b: ByteArray): String = buildString(b.size * 2) { b.forEach { append("%02x".format(it.toInt() and 0xff)) } }
    fun unhex(s: String): ByteArray {
        require(s.length % 2 == 0 && s.all { it in '0'..'9' || it in 'a'..'f' }) { "geçersiz hex" }
        return ByteArray(s.length / 2) { s.substring(it * 2, it * 2 + 2).toInt(16).toByte() }
    }

    fun sha256(b: ByteArray): ByteArray = MessageDigest.getInstance("SHA-256").digest(b)
    fun sha256Hex(b: ByteArray) = hex(sha256(b))

    fun hmac(key: ByteArray, msg: ByteArray): ByteArray =
        Mac.getInstance("HmacSHA256").run { init(SecretKeySpec(key, "HmacSHA256")); doFinal(msg) }

    /** RFC 5869 HKDF-SHA256. */
    fun hkdf(ikm: ByteArray, salt: ByteArray, info: ByteArray, len: Int): ByteArray {
        val prk = hmac(if (salt.isEmpty()) ByteArray(32) else salt, ikm)
        val out = java.io.ByteArrayOutputStream()
        var t = ByteArray(0)
        var i = 1
        while (out.size() < len) {
            t = hmac(prk, t + info + byteArrayOf(i.toByte()))
            out.write(t); i++
        }
        return out.toByteArray().copyOf(len)
    }

    /** Anahtar: HKDF(ikm = kod, salt, info = "muzikit-sync-v1|" + telefonId + "|" + pcId, 32). */
    fun deriveKey(code: String, salt: ByteArray, phoneId: String, pcId: String): ByteArray =
        hkdf(normalize(code).toByteArray(Charsets.UTF_8), salt, "muzikit-sync-v1|$phoneId|$pcId".toByteArray(), 32)

    fun requestText(method: String, path: String, time: Long, nonce: String, bodySha: String) =
        listOf("req", method, path, time.toString(), nonce, bodySha).joinToString("\n")

    fun responseText(status: Int, path: String, time: Long, requestNonce: String, bodySha: String) =
        listOf("resp", status.toString(), path, time.toString(), requestNonce, bodySha).joinToString("\n")

    fun sign(key: ByteArray, text: String) = hex(hmac(key, text.toByteArray(Charsets.UTF_8)))

    /** Sabit zamanlı karşılaştırma. */
    fun verify(key: ByteArray, text: String, authHex: String): Boolean =
        MessageDigest.isEqual(sign(key, text).toByteArray(), authHex.lowercase().toByteArray())

    /** Zaman (±60 sn) ve tekrar (120 sn) denetimi. */
    class ReplayGuard(private val windowSec: Long = 60, private val rememberSec: Long = 120) {
        private val seen = LinkedHashMap<String, Long>()
        @Synchronized
        fun accept(nonce: String, time: Long, now: Long): Boolean {
            if (nonce.length != 32 || !nonce.all { it in '0'..'9' || it in 'a'..'f' }) return false
            if (kotlin.math.abs(now - time) > windowSec) return false
            seen.entries.removeAll { now - it.value > rememberSec }
            if (nonce in seen) return false
            seen[nonce] = now
            return true
        }
    }
}
