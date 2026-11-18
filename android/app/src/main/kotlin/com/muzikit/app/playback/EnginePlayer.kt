package com.muzikit.app.playback

import androidx.media3.common.ForwardingPlayer
import androidx.media3.common.Player
import androidx.media3.common.util.UnstableApi
import com.muzikit.app.MusicHub

/**
 * Oynatıcıda her an tek şarkı yüklüdür; "sonraki/önceki" kararını [MusicHub] (masaüstü kuralları:
 * kuyruk, tekrarsız karıştırma, döngü, 3 sn kuralı) verir. Bu sarmalayıcı sayesinde bildirim,
 * kilit ekranı, kulaklık/araba tuşları da aynı kurallardan geçer.
 */
@UnstableApi
class EnginePlayer(player: Player) : ForwardingPlayer(player) {

    private val navigation = Player.Commands.Builder()
        .addAll(
            COMMAND_SEEK_TO_NEXT, COMMAND_SEEK_TO_NEXT_MEDIA_ITEM,
            COMMAND_SEEK_TO_PREVIOUS, COMMAND_SEEK_TO_PREVIOUS_MEDIA_ITEM,
            COMMAND_PLAY_PAUSE,
        )
        .build()

    override fun getAvailableCommands(): Player.Commands =
        super.getAvailableCommands().buildUpon().addAll(navigation).build()

    override fun isCommandAvailable(command: Int): Boolean =
        navigation.contains(command) || super.isCommandAvailable(command)

    override fun hasNextMediaItem(): Boolean = true
    override fun hasPreviousMediaItem(): Boolean = true

    override fun seekToNext() = MusicHub.next(manual = true)
    override fun seekToNextMediaItem() = MusicHub.next(manual = true)
    override fun seekToPrevious() = MusicHub.prev()
    override fun seekToPreviousMediaItem() = MusicHub.prev()

    /** Hiçbir şey yüklü değilken (ör. kulaklık "oynat" tuşu) son şarkıyı / ilk şarkıyı başlat. */
    override fun play() {
        // Sadece kulaklık modu: kulaklık yoksa hiçbir yoldan (uygulama, bildirim, tuş) çalmaz.
        if (!MusicHub.allowPlay()) return
        if (mediaItemCount == 0) MusicHub.startSomething() else super.play()
    }
}
