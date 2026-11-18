package com.muzikit.app.sync

import android.content.Context
import android.net.Uri
import android.provider.DocumentsContract
import java.security.MessageDigest
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale
import java.util.TimeZone

/** Defter satırı (`.muzikit-index.tsv`). [uri] sadece çalışma anında. */
data class IndexRow(
    val number: Int,
    val sha256: String,
    val size: Long,
    val mtime: Long,
    val videoId: String?,
    val addedAt: String,
    val addedBy: String,
    val name: String,
    val uri: Uri? = null,
) {
    fun toEntry() = SyncAlgorithm.Entry(number, sha256, addedAt, addedBy, name)
}

/** Defter metin biçimi — Android'den bağımsız, testli. */
object IndexFormat {
    const val FILE = ".muzikit-index.tsv"
    private const val HEADER = "# muzikit-index v1\nnumber\tsha256\tsize\tmtime\tvideoId\taddedAt\taddedBy\tname"
    val NAME = Regex("""^(\d{4}) - .*\.opus$""", RegexOption.DOT_MATCHES_ALL)

    fun utc(epochMs: Long): String =
        SimpleDateFormat("yyyy-MM-dd'T'HH:mm:ss'Z'", Locale.ROOT).apply { timeZone = TimeZone.getTimeZone("UTC") }
            .format(Date(epochMs))

    fun write(rows: List<IndexRow>): String = buildString {
        append(HEADER).append('\n')
        rows.sortedWith(compareBy({ it.number }, { it.name })).forEach {
            append(it.number.toString().padStart(4, '0')).append('\t').append(it.sha256).append('\t')
                .append(it.size).append('\t').append(it.mtime).append('\t').append(it.videoId ?: "-").append('\t')
                .append(it.addedAt).append('\t').append(it.addedBy).append('\t').append(it.name).append('\n')
        }
    }

    /** Bozuk satırlar atlanır (defter bir önbellek; sonra tarama düzeltir). */
    fun parse(text: String?): List<IndexRow> = text.orEmpty().lineSequence().mapNotNull { line ->
        if (line.isBlank() || line.startsWith("#") || line.startsWith("number\t")) return@mapNotNull null
        val f = line.split('\t')
        if (f.size < 8) return@mapNotNull null
        val name = f.subList(7, f.size).joinToString("\t")
        runCatching {
            IndexRow(
                number = f[0].toInt(), sha256 = f[1].also { require(it.length == 64) }, size = f[2].toLong(),
                mtime = f[3].toLong(), videoId = f[4].takeIf { it != "-" }, addedAt = f[5], addedBy = f[6], name = name,
            )
        }.getOrNull()
    }.toList()
}

/** SAF klasöründe defteri tarar/tamamlar. Klasör esas; defter önbellek. */
class SyncIndex(private val context: Context, private val tree: Uri, private val selfName: String = "phone") {

    private val cr = context.contentResolver
    private val treeDoc = DocumentsContract.buildDocumentUriUsingTree(tree, DocumentsContract.getTreeDocumentId(tree))

    private data class FileInfo(val name: String, val uri: Uri, val size: Long, val mtimeMs: Long)

    private fun listing(): Map<String, FileInfo> {
        val out = HashMap<String, FileInfo>(1024)
        val children = DocumentsContract.buildChildDocumentsUriUsingTree(tree, DocumentsContract.getTreeDocumentId(tree))
        cr.query(
            children,
            arrayOf(
                DocumentsContract.Document.COLUMN_DOCUMENT_ID, DocumentsContract.Document.COLUMN_DISPLAY_NAME,
                DocumentsContract.Document.COLUMN_SIZE, DocumentsContract.Document.COLUMN_LAST_MODIFIED,
            ),
            null, null, null,
        )?.use { c ->
            while (c.moveToNext()) {
                val id = c.getString(0) ?: continue
                val name = c.getString(1) ?: continue
                out[name] = FileInfo(name, DocumentsContract.buildDocumentUriUsingTree(tree, id), c.getLong(2), c.getLong(3))
            }
        }
        return out
    }

    fun uriOf(name: String): Uri? = listing()[name]?.uri

    /**
     * Klasörü tarar, deftere girmemiş şarkıları ekler, olmayanları düşer, değişeni yeniden hash'ler.
     * [videoIds]: indiricinin bildirdiği ad → video ID. [progress]: (yapılan, toplam) — sadece yeni hash'lerde.
     */
    fun scan(videoIds: Map<String, String> = emptyMap(), progress: (Int, Int) -> Unit = { _, _ -> }): List<IndexRow> {
        val files = listing()
        val old = IndexFormat.parse(files[IndexFormat.FILE]?.let { read(it.uri) }).associateBy { it.name }
        val songs = files.values.filter { IndexFormat.NAME.matches(it.name) }
        val toHash = songs.filter { f -> old[f.name].let { it == null || it.size != f.size || it.mtime != f.mtimeMs / 1000 } }
        var done = 0
        val rows = songs.map { f ->
            val prev = old[f.name]
            val num = IndexFormat.NAME.find(f.name)!!.groupValues[1].toInt()
            if (prev != null && prev.size == f.size && prev.mtime == f.mtimeMs / 1000) {
                prev.copy(number = num, uri = f.uri, videoId = prev.videoId ?: videoIds[f.name])
            } else {
                val sha = hash(f.uri)
                progress(++done, toHash.size)
                IndexRow(
                    number = num, sha256 = sha, size = f.size, mtime = f.mtimeMs / 1000,
                    videoId = videoIds[f.name] ?: prev?.videoId,
                    // Aynı ad + aynı içerik değilse yeni kayıt: dosya zamanı
                    addedAt = prev?.takeIf { it.sha256 == sha }?.addedAt ?: IndexFormat.utc(f.mtimeMs),
                    addedBy = prev?.takeIf { it.sha256 == sha }?.addedBy ?: selfName,
                    name = f.name, uri = f.uri,
                )
            }
        }
        val text = IndexFormat.write(rows)
        if (text != files[IndexFormat.FILE]?.let { read(it.uri) }) save(text, files[IndexFormat.FILE]?.uri)
        return rows
    }

    /** Gelen dosyanın satırını (karşı defterden) ekler/günceller. */
    fun upsert(row: IndexRow) {
        val files = listing()
        val rows = IndexFormat.parse(files[IndexFormat.FILE]?.let { read(it.uri) }).filter { it.name != row.name } + row
        save(IndexFormat.write(rows), files[IndexFormat.FILE]?.uri)
    }

    /** Kayma: satırın adını/numarasını değiştir (sha, addedAt, addedBy aynı kalır). */
    fun renameRow(oldName: String, newName: String, newNumber: Int, newMtime: Long) {
        val files = listing()
        val rows = IndexFormat.parse(files[IndexFormat.FILE]?.let { read(it.uri) })
            .map { if (it.name == oldName) it.copy(name = newName, number = newNumber, mtime = newMtime) else it }
        save(IndexFormat.write(rows), files[IndexFormat.FILE]?.uri)
    }

    private fun read(uri: Uri): String? = runCatching {
        cr.openInputStream(uri)?.use { it.readBytes().toString(Charsets.UTF_8) }
    }.getOrNull()

    private fun save(text: String, existing: Uri?) {
        val uri = existing ?: DocumentsContract.createDocument(cr, treeDoc, "application/octet-stream", IndexFormat.FILE) ?: return
        cr.openOutputStream(uri, "wt")?.use { it.write(text.toByteArray(Charsets.UTF_8)) }
    }

    fun hash(uri: Uri): String {
        val md = MessageDigest.getInstance("SHA-256")
        cr.openInputStream(uri)!!.use { input ->
            val buf = ByteArray(1 shl 16)
            while (true) {
                val n = input.read(buf); if (n < 0) break
                md.update(buf, 0, n)
            }
        }
        return SyncCrypto.hex(md.digest())
    }
}
