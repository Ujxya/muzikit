package com.muzikit.app.spectrum

import androidx.media3.common.C
import androidx.media3.common.audio.AudioProcessor
import androidx.media3.common.audio.BaseAudioProcessor
import androidx.media3.common.util.UnstableApi
import java.nio.ByteBuffer
import java.nio.ByteOrder

/**
 * Son ~2 sn'lik mono ses örnekleri ve her örneğin şarkıdaki konumu (Android'den bağımsız, testli).
 *
 * Oynatıcı sesi çalmadan önce (tampon kadar ileriden) çözer; analizör "şu an duyulanı" göstermeli.
 * Bu yüzden örnekler şarkıdaki konumlarıyla saklanır ve ekran, oynatıcının o anki konumuna
 * karşılık gelen pencereyi ister → çubuklar sesin önünden gitmez.
 */
class SampleRing(private val capacity: Int = 96_000) {
    private val buf = FloatArray(capacity)
    /** Son flush'tan beri yazılan toplam örnek (kare). */
    private var written = 0L
    private var baseUs = 0L
    var rate = 0
        private set

    @Synchronized
    fun reset(positionUs: Long, sampleRate: Int) {
        written = 0; baseUs = positionUs; rate = sampleRate
    }

    @Synchronized
    fun write(x: Float) {
        buf[(written % capacity).toInt()] = x
        written++
    }

    /**
     * Şarkıdaki [positionUs] anında biten [n] örneği [out]'a kopyalar. Döner: kopyalanan örnek sayısı
     * (0 = o ana ait veri yok: henüz çözülmedi ya da çok eski).
     */
    @Synchronized
    fun read(positionUs: Long, out: FloatArray, n: Int): Int {
        if (rate <= 0 || written == 0L) return 0
        var end = (positionUs - baseUs) * rate / 1_000_000L
        if (end > written) end = written           // konum biraz ilerdeyse en yeni veriye yasla
        val start = end - n
        if (start < 0 || start < written - capacity) return 0
        for (i in 0 until n) out[i] = buf[((start + i) % capacity).toInt()]
        return n
    }
}

/**
 * ExoPlayer ses hattına takılan musluk: sesi DEĞİŞTİRMEDEN geçirir, bir kopyasını [ring]'e yazar.
 * Mikrofon/Visualizer izni gerekmez. [enabled] kapalıyken (ekran kapalı / arka planda / analizör kapalı)
 * kopya alınmaz, sadece geçirilir.
 */
@UnstableApi
object SpectrumTap : BaseAudioProcessor() {

    val ring = SampleRing()

    @Volatile var enabled = false

    private var channels = 2
    private var float = false

    override fun onConfigure(inputAudioFormat: AudioProcessor.AudioFormat): AudioProcessor.AudioFormat {
        // Analizör ayarlardan kapalıysa musluk tamamen devre dışı: ses hiç kopyalanmaz (sonraki şarkıdan itibaren).
        if (!com.muzikit.app.MusicHub.settings.value.analyzer) return AudioProcessor.AudioFormat.NOT_SET
        if (inputAudioFormat.encoding != C.ENCODING_PCM_16BIT && inputAudioFormat.encoding != C.ENCODING_PCM_FLOAT) {
            return AudioProcessor.AudioFormat.NOT_SET // başka biçimde devre dışı (ses yine çalar)
        }
        channels = inputAudioFormat.channelCount
        float = inputAudioFormat.encoding == C.ENCODING_PCM_FLOAT
        return inputAudioFormat
    }

    override fun onFlush(streamMetadata: AudioProcessor.StreamMetadata) {
        // positionOffsetUs: flush'tan sonraki ilk örneğin şarkıdaki konumu (Media3 DefaultAudioSink)
        ring.reset(streamMetadata.positionOffsetUs, inputAudioFormat.sampleRate)
    }

    override fun queueInput(inputBuffer: ByteBuffer) {
        val size = inputBuffer.remaining()
        if (size == 0) return
        if (enabled) copyToRing(inputBuffer.duplicate().order(ByteOrder.nativeOrder()))
        replaceOutputBuffer(size).put(inputBuffer).flip()
    }

    private fun copyToRing(b: ByteBuffer) {
        val ch = channels.coerceAtLeast(1)
        if (float) {
            val frames = b.remaining() / (4 * ch)
            repeat(frames) {
                var s = 0f
                repeat(ch) { s += b.float }
                ring.write(s / ch)
            }
        } else {
            val frames = b.remaining() / (2 * ch)
            repeat(frames) {
                var s = 0f
                repeat(ch) { s += b.short / 32768f }
                ring.write(s / ch)
            }
        }
    }
}
