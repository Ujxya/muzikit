package com.muzikit.app.sync

import android.content.Context
import android.net.ConnectivityManager
import android.net.Uri
import android.net.nsd.NsdManager
import android.net.nsd.NsdServiceInfo
import android.provider.DocumentsContract
import android.util.Log
import com.muzikit.app.MusicHub
import com.muzikit.app.TAG
import com.muzikit.app.sync.SyncAlgorithm.Side
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.json.JSONArray
import org.json.JSONObject
import java.io.BufferedInputStream
import java.io.File
import java.io.InputStream
import java.io.OutputStream
import java.net.Inet4Address
import java.net.InetAddress
import java.net.ServerSocket
import java.net.Socket
import java.net.SocketTimeoutException
import java.security.MessageDigest

data class SyncUiState(
    val running: Boolean = false,
    /** Eşleştirme kodu (`XXXX-XXXX-XXXX`) — sadece eşleşmemişken. */
    val code: String? = null,
    val address: String? = null,
    val paired: Boolean = false,
    val status: String = "",
    val log: List<String> = emptyList(),
)

/**
 * Telefon tarafı eşitleme sunucusu. Sadece Ayarlar'da "eşitlemeyi aç" denince ve en fazla
 * 10 dakika hareketsizlikle çalışır; yalnız yerel ağ adreslerinden gelen bağlantıları kabul eder.
 */
object SyncServer {
    const val PORT = 47821
    private const val IDLE_MS = 10 * 60_000L
    private const val MAX_JSON = 4 shl 20
    private const val MAX_FILE = 100L shl 20

    private val _state = MutableStateFlow(SyncUiState())
    val state: StateFlow<SyncUiState> = _state.asStateFlow()

    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private var job: Job? = null
    private var server: ServerSocket? = null
    private var nsd: NsdManager.RegistrationListener? = null
    private lateinit var app: Context

    // Eşleştirme durumu
    private var code: String? = null
    private var salt: ByteArray? = null
    private var failures = 0
    private var replay = SyncCrypto.ReplayGuard()

    // Oturum planı (POST /v1/plan sonrası)
    private class Session(
        val pcId: String,
        val phone: List<IndexRow>,
        val pc: Map<String, JSONObject>, // sha → PC defter satırı
        val plan: SyncAlgorithm.Plan,
        val downloaded: List<String>,
        val putDone: MutableSet<String> = HashSet(),
        val renameDone: MutableSet<String> = HashSet(),
    )
    private var session: Session? = null

    private fun prefs() = app.getSharedPreferences("sync", Context.MODE_PRIVATE)

    val phoneId: String
        get() = prefs().getString("phone_id", null) ?: SyncCrypto.randomHex(8).also {
            prefs().edit().putString("phone_id", it).apply()
        }

    private fun keyFor(pcId: String): ByteArray? = prefs().getString("key_$pcId", null)?.let(SyncCrypto::unhex)

    fun isPaired(context: Context): Boolean {
        app = context.applicationContext
        return prefs().all.keys.any { it.startsWith("key_") }
    }

    /**
     * "Eşitleme geçmişini sil": tüm eşleşme anahtarları, cihaz kimliği (yenisi üretilir), tekrar-koruma belleği ve
     * günlük silinir. Müzik, downloaded.txt ve defter önbelleğine dokunulmaz. Sonraki eşitlemede yeniden kod gerekir.
     */
    fun resetAll(context: Context) {
        app = context.applicationContext
        if (job?.isActive == true) stop("kapatıldı")
        prefs().edit().clear().commit()
        replay = SyncCrypto.ReplayGuard()
        code = null; salt = null; failures = 0; session = null
        _state.value = SyncUiState(status = "eşitleme geçmişi silindi")
    }

    // ── Başlat / durdur ──────────────────────────────────

    fun start(context: Context) {
        app = context.applicationContext
        if (job?.isActive == true) return
        val paired = isPaired(app)
        newCode() // yeni bir bilgisayar eşleşebilsin diye her açılışta kod (eşleşmiş bilgisayar kodsuz bağlanır)
        session = null
        val addr = localIpv4()
        _state.value = SyncUiState(
            running = true, code = code?.let(SyncCrypto::display), address = addr?.let { "$it:$PORT" },
            paired = paired, status = if (paired) "bilgisayarı bekliyor… (yeni bilgisayar için kodu gir)" else "bilgisayara kodu gir",
            log = _state.value.log.takeLast(50),
        )
        job = scope.launch { serve() }
        registerNsd()
    }

    fun stop(reason: String = "kapatıldı") {
        runCatching { server?.close() }
        server = null
        job?.cancel(); job = null
        nsd?.let { l -> runCatching { app.getSystemService(NsdManager::class.java).unregisterService(l) } }
        nsd = null
        code = null; salt = null; session = null
        _state.value = _state.value.copy(running = false, code = null, status = reason)
        log("eşitleme $reason")
    }

    private fun newCode() {
        code = SyncCrypto.newCode()
        salt = SyncCrypto.randomBytes(16)
        failures = 0
        _state.value = _state.value.copy(code = code?.let(SyncCrypto::display))
    }

    private fun log(s: String) {
        Log.i(TAG, "eşitle: $s")
        _state.value = _state.value.copy(log = (_state.value.log + s).takeLast(80))
    }

    private fun status(s: String) { _state.value = _state.value.copy(status = s) }

    private fun localIpv4(): String? {
        val cm = app.getSystemService(ConnectivityManager::class.java)
        val lp = cm.getLinkProperties(cm.activeNetwork) ?: return null
        return lp.linkAddresses.map { it.address }.firstOrNull { it is Inet4Address && it.isSiteLocalAddress }?.hostAddress
    }

    private fun registerNsd() {
        val info = NsdServiceInfo().apply {
            serviceName = "muzikit-" + phoneId.take(6)
            serviceType = "_muzikit._tcp"
            port = PORT
            setAttribute("proto", "1")
        }
        val l = object : NsdManager.RegistrationListener {
            override fun onServiceRegistered(i: NsdServiceInfo) { log("ağda görünür: ${i.serviceName}") }
            override fun onRegistrationFailed(i: NsdServiceInfo, e: Int) { log("ağ duyurusu başarısız ($e); IP ile bağlanılabilir") }
            override fun onServiceUnregistered(i: NsdServiceInfo) {}
            override fun onUnregistrationFailed(i: NsdServiceInfo, e: Int) {}
        }
        nsd = l
        runCatching { app.getSystemService(NsdManager::class.java).registerService(info, NsdManager.PROTOCOL_DNS_SD, l) }
    }

    // ── Sunucu döngüsü ───────────────────────────────────

    private fun serve() {
        try {
            val ss = ServerSocket(PORT).also { it.soTimeout = 5_000; it.reuseAddress = true }
            server = ss
            log("dinleniyor: ${_state.value.address ?: "port $PORT"}")
            var lastActivity = System.currentTimeMillis()
            while (job?.isActive == true) {
                if (System.currentTimeMillis() - lastActivity > IDLE_MS) { stop("10 dk hareketsiz: kapandı"); return }
                val s = try { ss.accept() } catch (_: SocketTimeoutException) { continue }
                lastActivity = System.currentTimeMillis()
                s.use { handle(it) }
            }
        } catch (e: Exception) {
            if (job?.isActive == true) { Log.e(TAG, "eşitleme sunucusu", e); stop("hata: ${e.message}") }
        }
    }

    private class Req(val method: String, val target: String, val headers: Map<String, String>, val input: InputStream)

    private fun handle(sock: Socket) {
        sock.soTimeout = 60_000
        val remote = sock.inetAddress
        if (!isLocal(remote)) { log("reddedildi (yerel ağ değil): ${remote.hostAddress}"); return }
        val input = BufferedInputStream(sock.getInputStream())
        val out = sock.getOutputStream()
        val req = readHead(input) ?: return
        try {
            route(req, out)
        } catch (e: HttpError) {
            send(out, e.code, JSONObject().put("error", e.msg).toString().toByteArray(), null, null, req.target)
        } catch (e: Exception) {
            Log.e(TAG, "eşitle istek", e)
            send(out, 500, JSONObject().put("error", "internal").toString().toByteArray(), null, null, req.target)
        }
    }

    private fun isLocal(a: InetAddress) = a.isSiteLocalAddress || a.isLinkLocalAddress || a.isLoopbackAddress

    private fun readHead(input: InputStream): Req? {
        val buf = java.io.ByteArrayOutputStream()
        var last4 = 0
        while (true) {
            val b = input.read(); if (b < 0) return null
            buf.write(b)
            last4 = (last4 shl 8) or b
            if (last4 == 0x0d0a0d0a) break
            if (buf.size() > 16 * 1024) return null
        }
        val lines = buf.toString("UTF-8").split("\r\n")
        val parts = lines[0].split(" ")
        if (parts.size != 3) return null
        val h = HashMap<String, String>()
        lines.drop(1).filter { ':' in it }.forEach { val i = it.indexOf(':'); h[it.substring(0, i).trim().lowercase()] = it.substring(i + 1).trim() }
        return Req(parts[0], parts[1], h, input)
    }

    private class HttpError(val code: Int, val msg: String) : Exception(msg)

    // ── Yönlendirme ──────────────────────────────────────

    private fun route(r: Req, out: OutputStream) {
        val path = r.target.substringBefore('?')
        if (r.method == "GET" && path == "/v1/hello") return hello(r, out)

        // Buradan sonrası imzalı
        val pcId = r.headers["x-muzikit-device"]?.takeIf { it.length in 1..64 && it.all(Char::isLetterOrDigit) }
            ?: throw HttpError(401, "device")
        val time = r.headers["x-muzikit-time"]?.toLongOrNull() ?: throw HttpError(401, "time")
        val nonce = r.headers["x-muzikit-nonce"] ?: throw HttpError(401, "nonce")
        val auth = r.headers["x-muzikit-auth"] ?: throw HttpError(401, "auth")
        val pairing = keyFor(pcId) == null
        // Eşleşirken yalnız GET /v1/index — kodu bilmeyen biri gövde gönderip yer dolduramasın.
        if (pairing && (r.method != "GET" || path != "/v1/index")) throw HttpError(401, "pair-with-index")
        val key = keyFor(pcId) ?: run {
            val c = code ?: throw HttpError(401, "not-paired"); val s = salt ?: throw HttpError(401, "not-paired")
            SyncCrypto.deriveKey(c, s, phoneId, pcId)
        }
        // Gövde (PUT'ta dosya geçici dosyaya akar, SHA hesaplanır)
        val len = r.headers["content-length"]?.toLongOrNull() ?: 0L
        var bodyBytes = ByteArray(0)
        var bodyFile: File? = null
        var plainSha = ""
        val bodySha: String
        if (r.method == "PUT") {
            if (len > MAX_FILE) throw HttpError(413, "too-large")
            val f = File(app.cacheDir, "sync-upload.part").also { it.delete() }
            val md = MessageDigest.getInstance("SHA-256")
            f.outputStream().use { o -> copyN(r.input, o, len, md) }
            bodyFile = f; bodySha = SyncCrypto.hex(md.digest())
        } else {
            if (len > MAX_JSON) throw HttpError(413, "too-large")
            bodyBytes = ByteArray(len.toInt()).also { readFully(r.input, it) }
            bodySha = SyncCrypto.sha256Hex(bodyBytes)
        }
        val now = System.currentTimeMillis() / 1000
        val text = SyncCrypto.requestText(r.method, r.target, time, nonce, bodySha)
        if (SyncCrypto.verify(key, text, auth) && !replay.accept(nonce, time, now)) {
            bodyFile?.delete()
            Signed(key, SyncCipher.encKey(key), nonce, r.target).json(out, 401, JSONObject().put("error", "stale")) // imzalı: anahtar geçerli
            return
        }
        if (!SyncCrypto.verify(key, text, auth)) {
            bodyFile?.delete()
            if (pairing) {
                failures++
                log("eşleştirme denemesi başarısız ($failures/3)")
                if (failures >= 3) { newCode(); log("3 hatalı deneme: yeni kod üretildi") }
            }
            throw HttpError(401, "bad-signature")
        }
        if (pairing) {
            prefs().edit().putString("key_$pcId", SyncCrypto.hex(key)).apply()
            newCode() // kullanılan kod bir daha geçmesin
            _state.value = _state.value.copy(paired = true, status = "eşleşti")
            log("bilgisayarla eşleşildi")
        }
        val signed = Signed(key, SyncCipher.encKey(key), nonce, r.target)
        // POST/PUT gövdesi şifreli akış → çöz. GET gövdesiz olmalı.
        try {
            val ctx = SyncCipher.requestContext(r.method, r.target, time, nonce)
            when {
                r.method == "GET" -> if (len != 0L) throw HttpError(400, "get-body")
                r.method == "PUT" -> {
                    val plainFile = File(app.cacheDir, "sync-upload.plain").also { it.delete() }
                    val md = MessageDigest.getInstance("SHA-256")
                    bodyFile!!.inputStream().use { inp ->
                        java.security.DigestOutputStream(plainFile.outputStream(), md).use { o ->
                            SyncCipher.decrypt(signed.enc, ctx, inp, len, o)
                        }
                    }
                    bodyFile.delete()
                    bodyFile = plainFile; plainSha = SyncCrypto.hex(md.digest())
                }
                else -> bodyBytes = SyncCipher.decryptBytes(signed.enc, ctx, bodyBytes)
            }
        } catch (e: SyncCipher.DecryptError) {
            bodyFile?.delete()
            signed.json(out, 400, JSONObject().put("error", "decrypt"))
            return
        } catch (e: HttpError) {
            bodyFile?.delete()
            signed.json(out, e.code, JSONObject().put("error", e.msg))
            return
        }
        try {
            when {
                r.method == "GET" && path == "/v1/index" -> signed.json(out, 200, indexJson())
                r.method == "POST" && path == "/v1/plan" -> signed.json(out, 200, makePlan(pcId, JSONObject(String(bodyBytes, Charsets.UTF_8))))
                r.method == "GET" && path.startsWith("/v1/file/") -> sendFile(signed, out, path.removePrefix("/v1/file/"))
                r.method == "PUT" && path.startsWith("/v1/file/") -> signed.json(out, 200, receiveFile(path.removePrefix("/v1/file/"), bodyFile!!, plainSha))
                r.method == "POST" && path == "/v1/rename" -> signed.json(out, 200, rename(JSONObject(String(bodyBytes, Charsets.UTF_8))))
                r.method == "POST" && path == "/v1/downloaded" -> signed.json(out, 200, writeDownloaded(JSONObject(String(bodyBytes, Charsets.UTF_8))))
                r.method == "POST" && path == "/v1/done" -> { signed.json(out, 200, JSONObject().put("ok", true)); done() }
                else -> throw HttpError(404, "not-found")
            }
        } catch (e: HttpError) {
            signed.json(out, e.code, JSONObject().put("error", e.msg))
        } finally {
            bodyFile?.delete()
        }
    }

    /** Android 10–12'de OutputStream.nullOutputStream() yok. */
    private object NullOut : OutputStream() {
        override fun write(b: Int) {}
        override fun write(b: ByteArray, off: Int, len: Int) {}
    }

    private class Signed(val key: ByteArray, val enc: ByteArray, val reqNonce: String, val target: String)

    private fun Signed.json(out: OutputStream, code: Int, o: JSONObject) {
        val b = o.toString().toByteArray(Charsets.UTF_8)
        sendEncrypted(out, code, "application/json; charset=utf-8", b.size.toLong(), this) { b.inputStream() }
    }

    /**
     * İmzalı + şifreli yanıt. İmza şifreli gövdenin SHA'sını kapsadığı ve başlıklar gövdeden önce gittiği için
     * aynı önekle iki geçiş yapılır: önce sadece özet, sonra gönderim (AES-GCM deterministik → aynı baytlar).
     */
    private fun sendEncrypted(out: OutputStream, code: Int, type: String, plainLen: Long, s: Signed, open: () -> InputStream) {
        val t = System.currentTimeMillis() / 1000
        val prefix = SyncCrypto.randomBytes(8)
        val ctx = SyncCipher.responseContext(code, s.target, t, s.reqNonce)
        val md = MessageDigest.getInstance("SHA-256")
        open().use { SyncCipher.encrypt(s.enc, ctx, it, plainLen, java.security.DigestOutputStream(NullOut, md), prefix) }
        val cipherSha = SyncCrypto.hex(md.digest())
        val head = StringBuilder("HTTP/1.1 $code ${reason(code)}\r\nContent-Type: $type\r\n")
            .append("Content-Length: ").append(SyncCipher.cipherLength(plainLen)).append("\r\nConnection: close\r\n")
            .append("X-Muzikit-Enc: ").append(SyncCipher.HEADER_VALUE).append("\r\n")
            .append("X-Muzikit-Device: ").append(phoneId).append("\r\n")
            .append("X-Muzikit-Time: ").append(t).append("\r\n")
            .append("X-Muzikit-Nonce: ").append(SyncCrypto.randomHex(16)).append("\r\n")
            .append("X-Muzikit-Auth: ").append(SyncCrypto.sign(s.key, SyncCrypto.responseText(code, s.target, t, s.reqNonce, cipherSha))).append("\r\n\r\n")
        out.write(head.toString().toByteArray(Charsets.UTF_8))
        open().use { SyncCipher.encrypt(s.enc, ctx, it, plainLen, out, prefix) }
        out.flush()
    }

    private fun send(out: OutputStream, code: Int, body: ByteArray, s: Signed?, bodySha: String?, target: String) {
        sendHead(out, code, body.size.toLong(), "application/json; charset=utf-8", s, bodySha, target)
        out.write(body); out.flush()
    }

    private fun sendHead(out: OutputStream, code: Int, len: Long, type: String, s: Signed?, bodySha: String?, target: String) {
        val sb = StringBuilder("HTTP/1.1 $code ${reason(code)}\r\nContent-Type: $type\r\nContent-Length: $len\r\nConnection: close\r\n")
        if (s != null && bodySha != null) {
            val t = System.currentTimeMillis() / 1000
            val n = SyncCrypto.randomHex(16)
            sb.append("X-Muzikit-Device: ").append(phoneId).append("\r\n")
                .append("X-Muzikit-Time: ").append(t).append("\r\n")
                .append("X-Muzikit-Nonce: ").append(n).append("\r\n")
                .append("X-Muzikit-Auth: ").append(SyncCrypto.sign(s.key, SyncCrypto.responseText(code, target, t, s.reqNonce, bodySha))).append("\r\n")
        }
        out.write(sb.append("\r\n").toString().toByteArray(Charsets.UTF_8))
    }

    private fun reason(c: Int) = when (c) { 200 -> "OK"; 400 -> "Bad Request"; 401 -> "Unauthorized"; 403 -> "Forbidden"
        404 -> "Not Found"; 409 -> "Conflict"; 413 -> "Payload Too Large"; else -> "Error" }

    // ── Uç noktalar ──────────────────────────────────────

    private fun hello(r: Req, out: OutputStream) {
        val pcId = Uri.parse("http://x" + r.target).getQueryParameter("device").orEmpty()
        val pairing = pcId.isEmpty() || keyFor(pcId) == null
        val o = JSONObject().put("proto", 2).put("device", phoneId).put("name", "Android telefon") // model yerel ağa duyurulmaz
            .put("pairing", pairing && code != null)
            .put("salt", if (pairing && salt != null) SyncCrypto.hex(salt!!) else JSONObject.NULL)
        send(out, 200, o.toString().toByteArray(), null, null, r.target)
        log("bilgisayar bağlandı (${if (pairing) "eşleştirme" else "eşleşmiş"})")
    }

    private fun folderIndex(): SyncIndex {
        val tree = MusicHub.library.value.folder ?: throw HttpError(409, "no-folder")
        if (!MusicHub.library.value.writable) throw HttpError(409, "folder-read-only")
        return SyncIndex(app, tree)
    }

    private fun currentRows(): List<IndexRow> {
        status("defter hazırlanıyor…")
        return folderIndex().scan(SyncVideoIds.all(app)) { d, n -> status("parmak izi: $d/$n") }
    }

    private fun downloadedList(): List<String> {
        val tree = MusicHub.library.value.folder ?: return emptyList()
        val f = com.muzikit.app.download.SafFolder(app, tree)
        val txt = f.list()["downloaded.txt"]?.let(f::readText).orEmpty()
        return txt.lineSequence().map { it.trim() }.filter { it.isNotEmpty() }.toList()
    }

    private fun indexJson(): JSONObject {
        val rows = currentRows()
        status("defter gönderildi (${rows.size} şarkı)")
        return JSONObject().put("device", phoneId).put("items", JSONArray().apply {
            rows.sortedBy { it.number }.forEach { put(rowJson(it)) }
        }).put("downloaded", JSONArray(downloadedList()))
    }

    private fun rowJson(r: IndexRow) = JSONObject().put("number", r.number).put("sha256", r.sha256).put("size", r.size)
        .put("videoId", r.videoId ?: JSONObject.NULL).put("addedAt", r.addedAt).put("addedBy", r.addedBy).put("name", r.name)

    private fun makePlan(pcId: String, body: JSONObject): JSONObject {
        val items = body.optJSONArray("items") ?: throw HttpError(400, "items")
        val pcRows = (0 until items.length()).map { items.getJSONObject(it) }
        pcRows.forEach {
            val n = it.optString("name")
            if (!IndexFormat.NAME.matches(n) || it.optString("sha256").length != 64) throw HttpError(400, "bad-item")
        }
        val pcEntries = pcRows.map { SyncAlgorithm.Entry(it.getInt("number"), it.getString("sha256"), it.getString("addedAt"), it.getString("addedBy"), it.getString("name")) }
        val phoneRows = currentRows()
        val plan = SyncAlgorithm.plan(phoneRows.map { it.toEntry() }, pcEntries)
        val pcDl = body.optJSONArray("downloaded")?.let { a -> (0 until a.length()).map { a.getString(it) } }.orEmpty()
            .filter { com.muzikit.app.download.UrlRules.isValidVideoId(it) }
        val merged = SyncAlgorithm.mergeDownloaded(pcDl, downloadedList())
        session = Session(pcId, phoneRows, pcRows.associateBy { it.getString("sha256") }, plan, merged)
        fun side(s: Side) = if (s == Side.A) "phone" else "pc"
        fun xfer(to: Side) = JSONArray().apply {
            plan.transfers.filter { it.to == to }.sortedBy { it.number }.forEach {
                put(JSONObject().put("sha256", it.entry.sha).put("number", it.number).put("name", SyncAlgorithm.retitle(it.entry.name, it.number)))
            }
        }
        val o = JSONObject()
            .put("toPhone", xfer(Side.A)).put("toPc", xfer(Side.B))
            .put("renames", JSONArray().apply {
                plan.renames.sortedWith(compareBy({ side(it.side) }, { it.from })).forEach { r ->
                    val src = (if (r.side == Side.A) phoneRows.first { it.sha256 == r.sha && it.number == r.from }.name
                        else pcRows.first { it.getString("sha256") == r.sha && it.getInt("number") == r.from }.getString("name"))
                    put(JSONObject().put("side", side(r.side)).put("sha256", r.sha).put("from", r.from).put("to", r.to)
                        .put("name", SyncAlgorithm.retitle(src, r.to)))
                }
            })
            .put("mismatch", JSONArray().apply { plan.mismatch.sortedBy { it.sha }.forEach { put(JSONObject().put("sha256", it.sha).put("phone", it.a).put("pc", it.b)) } })
            .put("duplicates", JSONArray().apply {
                plan.duplicates.sortedWith(compareBy({ side(it.side) }, { it.number })).forEach { put(JSONObject().put("side", side(it.side)).put("sha256", it.sha).put("number", it.number)) }
            })
            .put("downloaded", JSONArray(merged))
        val nToPhone = plan.transfers.count { it.to == Side.A }; val nToPc = plan.transfers.count { it.to == Side.B }
        log("plan: telefona $nToPhone, bilgisayara $nToPc, kayma ${plan.renames.size}, uyuşmazlık ${plan.mismatch.size}")
        val dlChanged = merged != downloadedList()
        if (nToPhone == 0 && nToPc == 0 && plan.renames.isEmpty() && !dlChanged) {
            status("✓ zaten eşit, yapılacak bir şey yok")
            log("✓ bilgisayarla zaten eşit")
        } else {
            status("eşitleniyor: telefona $nToPhone, bilgisayara $nToPc şarkı")
        }
        return o
    }

    private fun sendFile(s: Signed, out: OutputStream, sha: String) {
        val ss = session ?: throw HttpError(409, "no-plan")
        if (ss.plan.transfers.none { it.to == Side.B && it.entry.sha == sha }) throw HttpError(403, "not-in-plan")
        val row = ss.phone.firstOrNull { it.sha256 == sha } ?: throw HttpError(404, "missing")
        val uri = row.uri ?: throw HttpError(404, "missing")
        sendEncrypted(out, 200, "application/octet-stream", row.size, s) { app.contentResolver.openInputStream(uri)!! }
        log("→ bilgisayara: ${row.name}")
    }

    private fun receiveFile(sha: String, tmp: File, bodySha: String): JSONObject {
        val ss = session ?: throw HttpError(409, "no-plan")
        val t = ss.plan.transfers.firstOrNull { it.to == Side.A && it.entry.sha == sha } ?: throw HttpError(403, "not-in-plan")
        if (bodySha != sha) throw HttpError(400, "sha-mismatch")
        if (!ss.putDone.add(sha)) throw HttpError(409, "already")
        val name = SyncAlgorithm.retitle(t.entry.name, t.number)
        if (!IndexFormat.NAME.matches(name) || '/' in name || name.contains("..")) throw HttpError(400, "bad-name")
        val tree = MusicHub.library.value.folder ?: throw HttpError(409, "no-folder")
        val idx = SyncIndex(app, tree)
        if (idx.uriOf(name) != null) { ss.putDone.remove(sha); throw HttpError(409, "exists") }
        val cr = app.contentResolver
        val treeDoc = DocumentsContract.buildDocumentUriUsingTree(tree, DocumentsContract.getTreeDocumentId(tree))
        val part = ".${t.number.toString().padStart(4, '0')}.part"
        idx.uriOf(part)?.let { runCatching { DocumentsContract.deleteDocument(cr, it) } }
        val doc = DocumentsContract.createDocument(cr, treeDoc, "application/octet-stream", part) ?: throw HttpError(500, "create")
        try {
            cr.openOutputStream(doc, "w")!!.use { o -> tmp.inputStream().use { it.copyTo(o, 1 shl 16) } }
            if (idx.uriOf(name) != null) throw HttpError(409, "exists")
            val renamed = DocumentsContract.renameDocument(cr, doc, name) ?: throw HttpError(500, "rename")
            val meta = ss.pc[sha]
            idx.upsert(IndexRow(
                number = t.number, sha256 = sha, size = tmp.length(), mtime = System.currentTimeMillis() / 1000,
                videoId = meta?.optString("videoId")?.takeIf { it.isNotEmpty() && it != "null" },
                addedAt = meta?.getString("addedAt") ?: t.entry.addedAt, addedBy = meta?.getString("addedBy") ?: t.entry.addedBy,
                name = name, uri = renamed,
            ))
        } catch (e: Exception) {
            runCatching { DocumentsContract.deleteDocument(cr, doc) }
            ss.putDone.remove(sha)
            throw e
        }
        log("← telefona: $name")
        return JSONObject().put("ok", true)
    }

    private fun rename(body: JSONObject): JSONObject {
        val ss = session ?: throw HttpError(409, "no-plan")
        val sha = body.optString("sha256")
        val r = ss.plan.renames.firstOrNull { it.side == Side.A && it.sha == sha } ?: throw HttpError(403, "not-in-plan")
        if (!ss.renameDone.add(sha)) throw HttpError(409, "already")
        val row = ss.phone.first { it.sha256 == sha && it.number == r.from }
        val newName = SyncAlgorithm.retitle(row.name, r.to)
        val tree = MusicHub.library.value.folder ?: throw HttpError(409, "no-folder")
        val idx = SyncIndex(app, tree)
        if (idx.uriOf(newName) != null) throw HttpError(409, "exists")
        val uri = idx.uriOf(row.name) ?: throw HttpError(404, "missing")
        DocumentsContract.renameDocument(app.contentResolver, uri, newName) ?: throw HttpError(500, "rename")
        idx.renameRow(row.name, newName, r.to, row.mtime)
        scope.launch(Dispatchers.Main) { MusicHub.renameFile(row.name, newName) }
        log("kayma: ${row.name} → $newName")
        return JSONObject().put("ok", true)
    }

    private fun writeDownloaded(body: JSONObject): JSONObject {
        val ss = session ?: throw HttpError(409, "no-plan")
        val ids = body.optJSONArray("ids")?.let { a -> (0 until a.length()).map { a.getString(it) } } ?: throw HttpError(400, "ids")
        if (ids != ss.downloaded) throw HttpError(409, "plan-mismatch")
        val tree = MusicHub.library.value.folder ?: throw HttpError(409, "no-folder")
        val f = com.muzikit.app.download.SafFolder(app, tree)
        val uri = f.list()["downloaded.txt"] ?: DocumentsContract.createDocument(
            app.contentResolver, DocumentsContract.buildDocumentUriUsingTree(tree, DocumentsContract.getTreeDocumentId(tree)),
            "text/plain", "downloaded.txt",
        ) ?: throw HttpError(500, "create")
        app.contentResolver.openOutputStream(uri, "wt")!!.use { it.write((ids.joinToString("\n") + "\n").toByteArray()) }
        log("downloaded.txt: ${ids.size} kayıt")
        return JSONObject().put("ok", true)
    }

    private fun done() {
        log("✓ eşitleme tamamlandı")
        status("✓ eşitlendi")
        scope.launch(Dispatchers.Main) { MusicHub.reload() }
        scope.launch { kotlinx.coroutines.delay(300); withContext(Dispatchers.Main) { stop("tamamlandı") } }
    }

    // ── Akış yardımcıları ────────────────────────────────

    private fun readFully(input: InputStream, b: ByteArray) {
        var off = 0
        while (off < b.size) { val n = input.read(b, off, b.size - off); if (n < 0) throw HttpError(400, "short-body"); off += n }
    }

    private fun copyN(input: InputStream, out: OutputStream, len: Long, md: MessageDigest) {
        val buf = ByteArray(1 shl 16); var left = len
        while (left > 0) {
            val n = input.read(buf, 0, minOf(buf.size.toLong(), left).toInt())
            if (n < 0) throw HttpError(400, "short-body")
            out.write(buf, 0, n); md.update(buf, 0, n); left -= n
        }
    }
}
