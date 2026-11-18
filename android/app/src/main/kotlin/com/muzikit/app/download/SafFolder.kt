package com.muzikit.app.download

import android.content.Context
import android.net.Uri
import android.provider.DocumentsContract
import java.io.File

/** Seçilen müzik klasörüne (SAF) okuma/yazma yardımcıları. */
class SafFolder(private val context: Context, private val tree: Uri) {

    private val cr = context.contentResolver
    private val treeDoc = DocumentsContract.buildDocumentUriUsingTree(tree, DocumentsContract.getTreeDocumentId(tree))

    fun canWrite(): Boolean = cr.persistedUriPermissions.any { it.uri == tree && it.isWritePermission }

    /** Klasördeki dosya adı → belge adresi. */
    fun list(): Map<String, Uri> {
        val out = HashMap<String, Uri>(1024)
        val children = DocumentsContract.buildChildDocumentsUriUsingTree(tree, DocumentsContract.getTreeDocumentId(tree))
        cr.query(
            children,
            arrayOf(DocumentsContract.Document.COLUMN_DOCUMENT_ID, DocumentsContract.Document.COLUMN_DISPLAY_NAME),
            null, null, null,
        )?.use { c ->
            while (c.moveToNext()) {
                val id = c.getString(0) ?: continue
                val name = c.getString(1) ?: continue
                out[name] = DocumentsContract.buildDocumentUriUsingTree(tree, id)
            }
        }
        return out
    }

    fun readText(uri: Uri): String? = runCatching {
        cr.openInputStream(uri)?.use { it.readBytes().toString(Charsets.UTF_8) }
    }.getOrNull()

    /**
     * [src] dosyasını klasöre [name] adıyla kopyalar. Ad zaten varsa ÜZERİNE YAZMAZ (null döner).
     * MIME "application/octet-stream": sağlayıcı ".opus" uzantısını değiştirmesin diye.
     * Dönen: oluşan dosyanın gerçek adı.
     */
    fun copyIn(src: File, name: String): String? {
        if (name in list()) return null
        val doc = DocumentsContract.createDocument(cr, treeDoc, "application/octet-stream", name) ?: return null
        try {
            cr.openOutputStream(doc, "w")!!.use { out -> src.inputStream().use { it.copyTo(out) } }
        } catch (e: Exception) {
            runCatching { DocumentsContract.deleteDocument(cr, doc) }
            throw e
        }
        return queryName(doc)
    }

    private fun queryName(doc: Uri): String? =
        cr.query(doc, arrayOf(DocumentsContract.Document.COLUMN_DISPLAY_NAME), null, null, null)?.use {
            if (it.moveToFirst()) it.getString(0) else null
        }

    /** downloaded.txt'nin sonuna bir satır ekler (yoksa oluşturur). Sadece geçerli ID yazılır. */
    fun appendDownloaded(videoId: String) {
        require(UrlRules.isValidVideoId(videoId)) { "geçersiz video kimliği downloaded.txt'ye yazılmadı" }
        val uri = list()["downloaded.txt"]
            ?: DocumentsContract.createDocument(cr, treeDoc, "text/plain", "downloaded.txt")
            ?: error("downloaded.txt oluşturulamadı")
        cr.openOutputStream(uri, "wa")!!.use { it.write("$videoId\n".toByteArray()) }
    }
}
