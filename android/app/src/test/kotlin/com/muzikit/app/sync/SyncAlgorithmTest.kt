package com.muzikit.app.sync

import com.muzikit.app.sync.SyncAlgorithm.Entry
import com.muzikit.app.sync.SyncAlgorithm.Side
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Test
import java.io.File

/** Masaüstüyle ortak `sync-test-vectors.json` varsa her vektör iki yönde denenir; dosya yoksa atlanır. */
class SyncAlgorithmTest {

    private val vectors: JSONObject by lazy {
        val f = listOf(File("../../sync-test-vectors.json"), File("../sync-test-vectors.json"), File("sync-test-vectors.json"))
            .firstOrNull { it.isFile }
        org.junit.Assume.assumeTrue("sync-test-vectors.json yok, atlandı", f != null)
        JSONObject(f!!.readText())
    }

    private fun entries(arr: JSONArray) = (0 until arr.length()).map {
        val e = arr.getJSONArray(it)
        Entry(e.getInt(0), e.getString(1), e.getString(2), e.getString(3))
    }

    private fun mapOf(o: JSONObject) = o.keys().asSequence().associate { it.toInt() to o.getString(it) }

    private fun run(case: JSONObject, swapped: Boolean) {
        val name = case.getString("name") + if (swapped) " [L↔R]" else ""
        val l = entries(case.getJSONArray("L")); val r = entries(case.getJSONArray("R"))
        val ex = case.getJSONObject("expect")
        // a = yerel (L), b = karşı (R); ters yönde L ve R yer değiştirir
        val (a, b) = if (swapped) r to l else l to r
        val sideOfL = if (swapped) Side.B else Side.A
        val sideOfR = sideOfL.other
        val p = SyncAlgorithm.plan(a, b)

        assertEquals("$name: L son durum", mapOf(ex.getJSONObject("L")), p.finalState(sideOfL, l))
        assertEquals("$name: R son durum", mapOf(ex.getJSONObject("R")), p.finalState(sideOfR, r))

        val expRen = ex.getJSONArray("renames").let { arr ->
            (0 until arr.length()).map { arr.getJSONObject(it) }.map {
                Triple(if (it.getString("side") == "L") sideOfL else sideOfR, it.getString("sha"), it.getInt("from") to it.getInt("to"))
            }.toSet()
        }
        assertEquals("$name: renames", expRen, p.renames.map { Triple(it.side, it.sha, it.from to it.to) }.toSet())

        val expMis = ex.getJSONArray("mismatch").let { arr ->
            (0 until arr.length()).map { arr.getJSONObject(it) }.map { Triple(it.getString("sha"), it.getInt("L"), it.getInt("R")) }.toSet()
        }
        val gotMis = p.mismatch.map { if (swapped) Triple(it.sha, it.b, it.a) else Triple(it.sha, it.a, it.b) }.toSet()
        assertEquals("$name: mismatch", expMis, gotMis)

        val expDup = ex.getJSONArray("duplicates").let { arr ->
            (0 until arr.length()).map { arr.getJSONObject(it) }.map {
                Triple(if (it.getString("side") == "L") sideOfL else sideOfR, it.getString("sha"), it.getInt("number"))
            }.toSet()
        }
        assertEquals("$name: duplicates", expDup, p.duplicates.map { Triple(it.side, it.sha, it.number) }.toSet())
    }

    @Test
    fun allVectorsBothDirections() {
        val cases = vectors.getJSONArray("cases")
        assertEquals(9, cases.length())
        for (i in 0 until cases.length()) {
            run(cases.getJSONObject(i), swapped = false)
            run(cases.getJSONObject(i), swapped = true)
        }
    }

    @Test
    fun downloadedMergePcOrderFirst() {
        assertEquals(listOf("p1", "p2", "x"), SyncAlgorithm.mergeDownloaded(listOf("p1", "p2"), listOf("x", "p2")))
    }

    @Test
    fun retitleKeepsTitle() {
        assertEquals("0818 - A - B.opus", SyncAlgorithm.retitle("0817 - A - B.opus", 818))
    }
}
