package com.muzikit.app.sync

/**
 * Telefon ↔ bilgisayar eşitleme algoritması (masaüstü Rust tarafıyla BİREBİR aynı).
 * Android'den bağımsız, `sync-test-vectors.json` ile iki yönde test edilir.
 *
 * Taraflar "a" ve "b" diye anılır (Android'de a = telefon, b = PC). Bütün kararlar iki girdi defterine
 * göre TEK SEFERDE verilir; MAX bir kez hesaplanır.
 */
object SyncAlgorithm {

    data class Entry(
        val number: Int,
        val sha: String,
        val addedAt: String,
        val addedBy: String,
        val name: String = "",
    ) {
        /** Sıralama anahtarı: (addedAt, addedBy, sha256) — küçük olan "eski". */
        val key: String get() = "$addedAt\u0000$addedBy\u0000$sha"
    }

    enum class Side { A, B; val other get() = if (this == A) B else A }

    data class Transfer(val to: Side, val entry: Entry, val number: Int)
    data class Rename(val side: Side, val sha: String, val from: Int, val to: Int)
    data class Mismatch(val sha: String, val a: Int, val b: Int)
    data class Duplicate(val side: Side, val sha: String, val number: Int)

    data class Plan(
        val transfers: List<Transfer>,
        val renames: List<Rename>,
        val mismatch: List<Mismatch>,
        val duplicates: List<Duplicate>,
    ) {
        /** Eşitleme sonrası bir tarafın numara → sha durumu (kopyalar dahil). */
        fun finalState(side: Side, start: List<Entry>): Map<Int, String> {
            val m = start.associate { it.number to it.sha }.toMutableMap()
            // Kopyalar aynı numarayı paylaşmaz ama aynı numarada iki dosya olabilir; burada sha esas.
            val all = start.map { it.number to it.sha }.toMutableList()
            renames.filter { it.side == side }.forEach { r ->
                val i = all.indexOfFirst { it.first == r.from && it.second == r.sha }
                if (i >= 0) all[i] = r.to to r.sha
            }
            transfers.filter { it.to == side }.forEach { all += it.number to it.entry.sha }
            m.clear(); all.forEach { (n, s) -> m[n] = s }
            return m
        }
    }

    fun plan(a: List<Entry>, b: List<Entry>): Plan {
        val dups = ArrayList<Duplicate>()
        fun canonical(side: Side, list: List<Entry>): Map<String, Entry> {
            val out = HashMap<String, Entry>()
            list.groupBy { it.sha }.forEach { (sha, es) ->
                val sorted = es.sortedBy { it.number }
                out[sha] = sorted.first()
                sorted.drop(1).forEach { dups += Duplicate(side, sha, it.number) }
            }
            return out
        }
        val ca = canonical(Side.A, a)
        val cb = canonical(Side.B, b)
        val max = (a.map { it.number } + b.map { it.number }).maxOrNull() ?: 0

        val mismatch = ArrayList<Mismatch>()
        ca.forEach { (sha, ea) -> cb[sha]?.let { eb -> if (ea.number != eb.number) mismatch += Mismatch(sha, ea.number, eb.number) } }

        val onlyA = ca.filterKeys { it !in cb }
        val onlyB = cb.filterKeys { it !in ca }
        val byNumberA = a.groupBy { it.number }
        val byNumberB = b.groupBy { it.number }
        val canonA = ca.values.toSet()
        val canonB = cb.values.toSet()

        val transfers = ArrayList<Transfer>()
        val renames = ArrayList<Rename>()
        // Yeni numara bekleyenler: (sahibi taraf, kayıt, kayma mı?)
        data class Pending(val side: Side, val e: Entry, val shift: Boolean)
        val pending = ArrayList<Pending>()
        val handled = HashSet<String>()

        fun process(side: Side, e: Entry) {
            if (!handled.add(side.name + e.sha)) return
            val otherNums = if (side == Side.A) byNumberB else byNumberA
            val otherOnly = if (side == Side.A) onlyB else onlyA
            val otherCanon = if (side == Side.A) canonB else canonA
            val occ = otherNums[e.number].orEmpty()
            when {
                occ.isEmpty() -> transfers += Transfer(side.other, e, e.number)
                occ.size == 1 && occ[0] in otherCanon && occ[0].sha in otherOnly -> {
                    // Çakışma: iki taraf aynı yeni numarayı farklı şarkıya vermiş
                    val x = occ[0]
                    handled.add(side.other.name + x.sha)
                    val (winner, wSide, loser, lSide) =
                        if (e.key <= x.key) Quad(e, side, x, side.other) else Quad(x, side.other, e, side)
                    transfers += Transfer(lSide, winner, winner.number) // kazanan karşıya aynı numarayla
                    pending += Pending(lSide, loser, shift = true)
                    @Suppress("UNUSED_VARIABLE") val unused = wSide
                }
                // Kopyanın ya da iki tarafta da olan şarkının tuttuğu numara: dolu, taşınmaz → yeni numara
                else -> pending += Pending(side, e, shift = false)
            }
        }
        onlyA.values.sortedBy { it.number }.forEach { process(Side.A, it) }
        onlyB.values.sortedBy { it.number }.forEach { process(Side.B, it) }

        var next = max + 1
        pending.sortedBy { it.e.key }.forEach { p ->
            val m = next++
            if (p.shift) {
                renames += Rename(p.side, p.e.sha, p.e.number, m)
                transfers += Transfer(p.side.other, p.e, m)
            } else {
                transfers += Transfer(p.side.other, p.e, m)
                mismatch += if (p.side == Side.A) Mismatch(p.e.sha, p.e.number, m) else Mismatch(p.e.sha, m, p.e.number)
            }
        }
        return Plan(
            transfers.sortedWith(compareBy({ it.to }, { it.number })),
            renames.sortedWith(compareBy({ it.side }, { it.from })),
            mismatch.sortedBy { it.sha },
            dups.sortedWith(compareBy({ it.side }, { it.number })),
        )
    }

    private data class Quad(val a: Entry, val b: Side, val c: Entry, val d: Side)

    /** downloaded.txt birleşimi; önce PC sırası, sonra PC'de olmayanlar telefon sırasıyla. */
    fun mergeDownloaded(pc: List<String>, phone: List<String>): List<String> =
        LinkedHashSet<String>().apply { addAll(pc); addAll(phone) }.toList()

    /** Aktarılan/kayan dosyanın adı: `NNNN - ` + kaynak adındaki başlık. */
    fun retitle(sourceName: String, number: Int): String {
        val title = Regex("""^\d{4} - (.*)$""", RegexOption.DOT_MATCHES_ALL).find(sourceName)?.groupValues?.get(1)
            ?: sourceName
        return number.toString().padStart(4, '0') + " - " + title
    }
}
