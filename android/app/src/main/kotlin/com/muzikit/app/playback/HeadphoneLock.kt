package com.muzikit.app.playback

/**
 * "Sadece kulaklık" kilidi — Android'den bağımsız karar mantığı (birim testli).
 *
 * İlke: ŞÜPHEDE SESSİZ. Mod açıkken uygulamanın sesi ancak medya sesinin şu an GERÇEKTEN kulaklığa
 * gittiği doğrulanınca açılır; boş/bilinmeyen/karışık yönlendirmede kısık kalır. Ek olarak medya sesi
 * telefonun hoparlörüne yönlendiğinde hoparlörün medya ses düzeyi 0'a çekilir (Android her çıkışın
 * sesini ayrı tuttuğu için kulaklığın ses düzeyi değişmez). Mod kapanınca eski hoparlör sesi geri verilir.
 */
object HeadphoneLock {

    data class Decision(
        /** Uygulamanın kendi ses çarpanı (0 = sessiz, 1 = normal). */
        val appVolume: Float,
        /** Çalıyorsa duraklatılmalı mı. */
        val pause: Boolean,
        /** Sistem medya sesi bu değere ayarlanmalı (null = dokunma). Sadece hoparlör aktifken verilir. */
        val setStreamVolume: Int?,
        /** Kaydedilecek "eski hoparlör sesi" (null = kayıt silinsin). */
        val savedSpeakerVolume: Int?,
    )

    /**
     * @param route medya sesinin şu an gittiği çıkış(lar); bilinmiyorsa boş
     * @param streamVolume şu anki (o çıkışa ait) medya ses düzeyi
     * @param savedSpeakerVolume daha önce kapatırken saklanan hoparlör sesi
     */
    fun decide(
        headphoneOnly: Boolean,
        route: List<Output>,
        isPlaying: Boolean,
        streamVolume: Int,
        savedSpeakerVolume: Int?,
    ): Decision {
        val speakerActive = route.any { it.kind == OutputKind.SPEAKER }
        if (!headphoneOnly) {
            // Mod kapalı: normal ses; hoparlörü biz kapattıysak, hoparlör aktif olduğu ilk anda geri ver.
            return if (speakerActive && savedSpeakerVolume != null) {
                Decision(1f, false, savedSpeakerVolume, null)
            } else {
                Decision(1f, false, null, savedSpeakerVolume)
            }
        }
        val routeOk = route.isNotEmpty() && route.all { it.isHeadphone }
        var set: Int? = null
        var saved = savedSpeakerVolume
        if (speakerActive && streamVolume > 0) {
            set = 0
            if (saved == null) saved = streamVolume
        }
        return Decision(
            appVolume = if (routeOk) 1f else 0f,
            pause = isPlaying && !routeOk,
            setStreamVolume = set,
            savedSpeakerVolume = saved,
        )
    }
}
