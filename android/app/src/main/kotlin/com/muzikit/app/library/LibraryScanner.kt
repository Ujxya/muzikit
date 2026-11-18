package com.muzikit.app.library

import android.content.Context
import android.net.Uri
import android.provider.DocumentsContract

/**
 * Kullanıcının seçtiği klasörü (Storage Access Framework) tarar.
 *
 * Neden SAF (MediaStore değil): dosya adı/numara düzeni masaüstüyle birebir aynı kalmalı ve
 * MediaStore başlığı etiketten okuyabilir, dosyayı geç indeksleyebilir. SAF dosya adını olduğu gibi verir.
 * `DocumentFile.listFiles()` yerine tek sorgu kullanılır: 800 dosyada her ad için ayrı sorgu yavaş olurdu.
 */
class ScanResult(val songs: List<Song>, val downloadedTxt: String?)

object LibraryScanner {

    fun scan(context: Context, treeUri: Uri): ScanResult {
        val treeDocId = DocumentsContract.getTreeDocumentId(treeUri)
        val childrenUri = DocumentsContract.buildChildDocumentsUriUsingTree(treeUri, treeDocId)
        val entries = ArrayList<Pair<String, String>>(1024)
        var downloadedUri: Uri? = null
        context.contentResolver.query(
            childrenUri,
            arrayOf(
                DocumentsContract.Document.COLUMN_DOCUMENT_ID,
                DocumentsContract.Document.COLUMN_DISPLAY_NAME,
                DocumentsContract.Document.COLUMN_MIME_TYPE,
            ),
            null, null, null,
        )?.use { c ->
            while (c.moveToNext()) {
                val id = c.getString(0) ?: continue
                val name = c.getString(1) ?: continue
                if (c.getString(2) == DocumentsContract.Document.MIME_TYPE_DIR) continue
                val uri = DocumentsContract.buildDocumentUriUsingTree(treeUri, id)
                if (name == "downloaded.txt") downloadedUri = uri
                entries += name to uri.toString()
            }
        }
        // Masaüstü indiricinin kaydı (rapor için; sadece okunur).
        val downloaded = downloadedUri?.let { u ->
            runCatching {
                context.contentResolver.openInputStream(u)?.use { s ->
                    s.readBytes().toString(Charsets.UTF_8)
                }
            }.getOrNull()
        }
        return ScanResult(buildLibrary(entries), downloaded)
    }
}
