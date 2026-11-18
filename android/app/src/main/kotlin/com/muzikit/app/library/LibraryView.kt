package com.muzikit.app.library

/** Sıralama: numara ↑ → numara ↓ → isim a-z (masaüstü `SortMode`). */
enum class SortMode(val label: String) {
    NUMBER("no ↑"), NUMBER_DESC("no ↓"), NAME("a-z");

    fun next(): SortMode = when (this) { NUMBER -> NUMBER_DESC; NUMBER_DESC -> NAME; NAME -> NUMBER }

    companion object {
        fun fromKey(k: String?): SortMode = entries.firstOrNull { it.name == k } ?: NUMBER
    }
}

/**
 * Görünür liste (masaüstü `rebuild_filter`): sadece favoriler filtresi, çok kelimeli arama
 * (her kelime küçük harfle geçmeli), sonra sıralama. [songs] dosya adına göre sıralı gelir.
 */
fun filterSongs(
    songs: List<Song>,
    query: String,
    onlyFavorites: Boolean,
    favorites: Set<String>,
    sort: SortMode,
): List<Song> {
    val terms = query.trim().lowercase().split(Regex("\\s+")).filter { it.isNotEmpty() }
    val list = songs.filter { s ->
        (!onlyFavorites || s.file in favorites) && terms.all { s.lower.contains(it) }
    }
    return when (sort) {
        SortMode.NUMBER -> list
        SortMode.NUMBER_DESC -> list.asReversed().toList()
        SortMode.NAME -> list.sortedBy { it.title.lowercase() }
    }
}
