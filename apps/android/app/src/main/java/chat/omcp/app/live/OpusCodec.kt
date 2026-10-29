package chat.omcp.app.live

import android.media.MediaCodec
import android.media.MediaFormat
import java.nio.ByteBuffer
import java.nio.ByteOrder

/**
 * Opus through the platform codecs (Android 10+ ships `c2.android.opus.encoder` /
 * `.decoder`, part of AOSP under Apache 2.0 with libopus under BSD-3-Clause), so no
 * third-party library is bundled. 48 kHz mono, 20 ms frames, 12 kbit/s (REQUIREMENTS "Live").
 */
object OpusFormat {
    const val SAMPLE_RATE = 48_000
    const val FRAME_MS = 20
    const val FRAME_SAMPLES = SAMPLE_RATE * FRAME_MS / 1000 // 960
    const val BITRATE = 12_000
    const val PRE_SKIP = 312

    /** RFC 7845 §5.1 identification header, the decoder's csd-0. */
    fun opusHead(channels: Int = 1, preSkip: Int = PRE_SKIP, rate: Int = SAMPLE_RATE): ByteArray =
        ByteBuffer.allocate(19).order(ByteOrder.LITTLE_ENDIAN).apply {
            put("OpusHead".toByteArray(Charsets.US_ASCII))
            put(1) // version
            put(channels.toByte())
            putShort(preSkip.toShort())
            putInt(rate)
            putShort(0) // output gain
            put(0) // mapping family
        }.array()

    /** csd-1 / csd-2: pre-skip and seek pre-roll in nanoseconds (native order, 8 bytes). */
    fun nanos(ns: Long): ByteArray = ByteBuffer.allocate(8).order(ByteOrder.nativeOrder()).putLong(ns).array()

    fun preSkipNs(preSkip: Int = PRE_SKIP): Long = preSkip * 1_000_000_000L / SAMPLE_RATE
    const val SEEK_PREROLL_NS = 80_000_000L
}

/** PCM16 mono 20 ms frames in, Opus packets out. */
class OpusEncoder(bitrate: Int = OpusFormat.BITRATE) : AutoCloseable {
    private val codec = MediaCodec.createEncoderByType(MediaFormat.MIMETYPE_AUDIO_OPUS)
    private var pts = 0L

    init {
        val f = MediaFormat.createAudioFormat(MediaFormat.MIMETYPE_AUDIO_OPUS, OpusFormat.SAMPLE_RATE, 1)
        f.setInteger(MediaFormat.KEY_BIT_RATE, bitrate)
        f.setInteger(MediaFormat.KEY_MAX_INPUT_SIZE, OpusFormat.FRAME_SAMPLES * 2)
        codec.configure(f, null, null, MediaCodec.CONFIGURE_FLAG_ENCODE)
        codec.start()
    }

    /** Feed one frame (960 samples); returns any packets ready (codec config is skipped). */
    fun encode(pcm: ShortArray): List<ByteArray> {
        val inIx = codec.dequeueInputBuffer(10_000)
        if (inIx >= 0) {
            val b = codec.getInputBuffer(inIx)!!
            b.clear()
            b.order(ByteOrder.nativeOrder()).asShortBuffer().put(pcm)
            codec.queueInputBuffer(inIx, 0, pcm.size * 2, pts, 0)
            pts += OpusFormat.FRAME_MS * 1000L
        }
        return drain()
    }

    private fun drain(): List<ByteArray> {
        val out = mutableListOf<ByteArray>()
        val info = MediaCodec.BufferInfo()
        while (true) {
            val ix = codec.dequeueOutputBuffer(info, 0)
            if (ix < 0) break
            if (info.flags and MediaCodec.BUFFER_FLAG_CODEC_CONFIG == 0 && info.size > 0) {
                val b = codec.getOutputBuffer(ix)!!
                val a = ByteArray(info.size)
                b.position(info.offset); b.get(a)
                out += a
            }
            codec.releaseOutputBuffer(ix, false)
        }
        return out
    }

    override fun close() { runCatching { codec.stop() }; codec.release() }
}

/** Opus packets in, PCM16 out. A lost frame is concealed by decoding the next packet (FEC) or silence. */
class OpusDecoder : AutoCloseable {
    private val codec = MediaCodec.createDecoderByType(MediaFormat.MIMETYPE_AUDIO_OPUS)
    private var pts = 0L

    init {
        val f = MediaFormat.createAudioFormat(MediaFormat.MIMETYPE_AUDIO_OPUS, OpusFormat.SAMPLE_RATE, 1)
        f.setByteBuffer("csd-0", ByteBuffer.wrap(OpusFormat.opusHead()))
        f.setByteBuffer("csd-1", ByteBuffer.wrap(OpusFormat.nanos(OpusFormat.preSkipNs())))
        f.setByteBuffer("csd-2", ByteBuffer.wrap(OpusFormat.nanos(OpusFormat.SEEK_PREROLL_NS)))
        codec.configure(f, null, null, 0)
        codec.start()
    }

    fun decode(packet: ByteArray): ShortArray {
        val inIx = codec.dequeueInputBuffer(10_000)
        if (inIx >= 0) {
            val b = codec.getInputBuffer(inIx)!!
            b.clear(); b.put(packet)
            codec.queueInputBuffer(inIx, 0, packet.size, pts, 0)
            pts += OpusFormat.FRAME_MS * 1000L
        }
        val info = MediaCodec.BufferInfo()
        var pcm = ShortArray(0)
        while (true) {
            val ix = codec.dequeueOutputBuffer(info, 2_000)
            if (ix < 0) break
            if (info.size > 0) {
                val b = codec.getOutputBuffer(ix)!!.order(ByteOrder.nativeOrder())
                b.position(info.offset); b.limit(info.offset + info.size)
                val s = ShortArray(info.size / 2)
                b.asShortBuffer().get(s)
                pcm += s
            }
            codec.releaseOutputBuffer(ix, false)
        }
        return pcm
    }

    override fun close() { runCatching { codec.stop() }; codec.release() }
}
