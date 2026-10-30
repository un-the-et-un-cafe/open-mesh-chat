package chat.omcp.app.live

import android.annotation.SuppressLint
import android.media.AudioAttributes
import android.media.AudioFormat
import android.media.AudioRecord
import android.media.AudioTrack
import android.media.MediaRecorder
import chat.omcp.app.core.LivePlayoutUi
import chat.omcp.app.core.OmcpCore

/**
 * Mic → Opus → Rust (`livePushAudio`) when hosting; Rust jitter buffer (`liveTakeAudio`)
 * → Opus → speaker when listening. Packets, keys, hop limit and loss handling live in Rust.
 * [kick] pushes the returned links onto the radio (BleMeshTransport).
 */
class LiveAudioEngine(private val core: OmcpCore, private val kick: (List<Long>) -> Unit) : chat.omcp.app.ui.vm.LiveAudio {
    @Volatile private var running = false
    private var thread: Thread? = null

    /** Caller must hold RECORD_AUDIO (asked on first Go live only). */
    @SuppressLint("MissingPermission")
    override fun startHosting() {
        stop()
        running = true
        thread = Thread({
            val minBuf = AudioRecord.getMinBufferSize(OpusFormat.SAMPLE_RATE, AudioFormat.CHANNEL_IN_MONO, AudioFormat.ENCODING_PCM_16BIT)
            val rec = AudioRecord(MediaRecorder.AudioSource.VOICE_COMMUNICATION, OpusFormat.SAMPLE_RATE,
                AudioFormat.CHANNEL_IN_MONO, AudioFormat.ENCODING_PCM_16BIT, maxOf(minBuf, OpusFormat.FRAME_SAMPLES * 4))
            val enc = OpusEncoder()
            val pcm = ShortArray(OpusFormat.FRAME_SAMPLES)
            try {
                rec.startRecording()
                while (running) {
                    var n = 0
                    while (n < pcm.size && running) {
                        val r = rec.read(pcm, n, pcm.size - n)
                        if (r <= 0) break
                        n += r
                    }
                    if (n < pcm.size) continue
                    val packets = enc.encode(pcm)
                    if (packets.isNotEmpty()) kick(core.livePushAudio(packets))
                }
            } finally {
                runCatching { rec.stop() }; rec.release(); enc.close()
            }
        }, "live-host").apply { isDaemon = true; start() }
    }

    override fun startListening() {
        stop()
        running = true
        thread = Thread({
            val track = AudioTrack.Builder()
                .setAudioAttributes(AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_MEDIA)
                    .setContentType(AudioAttributes.CONTENT_TYPE_SPEECH).build())
                .setAudioFormat(AudioFormat.Builder().setSampleRate(OpusFormat.SAMPLE_RATE)
                    .setChannelMask(AudioFormat.CHANNEL_OUT_MONO).setEncoding(AudioFormat.ENCODING_PCM_16BIT).build())
                .setTransferMode(AudioTrack.MODE_STREAM)
                .build()
            val dec = OpusDecoder()
            try {
                track.play()
                while (running) {
                    for (p in core.liveTakeAudio(System.currentTimeMillis())) {
                        val pcm = pcmFor(p, dec)
                        if (pcm.isNotEmpty()) track.write(pcm, 0, pcm.size)
                    }
                    Thread.sleep(40)
                }
            } catch (_: InterruptedException) {
            } finally {
                runCatching { track.stop() }; track.release(); dec.close()
            }
        }, "live-listen").apply { isDaemon = true; start() }
    }

    override fun stop() {
        running = false
        thread?.interrupt()
        thread = null
    }

    companion object {
        /**
         * Lost slot: 20 ms of silence. The platform decoder has no FEC-decode switch, so
         * `fecNext` is not used on Android yet (docs/LIVE.md); the next packet plays normally.
         */
        fun pcmFor(p: LivePlayoutUi, dec: OpusDecoder): ShortArray =
            if (p.lost) silence() else dec.decode(p.opus)

        fun silence() = ShortArray(OpusFormat.FRAME_SAMPLES)
    }
}
