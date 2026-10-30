import Foundation
import AVFoundation
import AudioToolbox
import HoplaKit

/// iOS Live audio (mesh): mic → Opus (AudioToolbox, 48 kHz mono, 20 ms, 12 kbit/s) →
/// `livePushAudio`; `liveTakeAudio` → Opus decode → speaker. Same raw Opus packets as
/// Android's MediaCodec path, so either platform hears the other.
final class LiveAudioIO {
    static let rate: Double = 48_000
    static let frame: AVAudioFrameCount = 960 // 20 ms
    private let core: OmcpCore
    private weak var kick: MeshKick?
    private let engine = AVAudioEngine()
    private let player = AVAudioPlayerNode()
    private let pcm = AVAudioFormat(commonFormat: .pcmFormatFloat32, sampleRate: LiveAudioIO.rate, channels: 1, interleaved: false)!
    private let opus: AVAudioFormat
    private var timer: DispatchSourceTimer?
    private let q = DispatchQueue(label: "omcp.live.audio")
    private var pendingSamples: [Float] = []
    private var encoder: AVAudioConverter?
    private var decoder: AVAudioConverter?
    private var toPcm: AVAudioConverter?

    init(core: OmcpCore, kick: MeshKick?) {
        self.core = core; self.kick = kick
        var d = AudioStreamBasicDescription(mSampleRate: LiveAudioIO.rate, mFormatID: kAudioFormatOpus, mFormatFlags: 0,
                                            mBytesPerPacket: 0, mFramesPerPacket: LiveAudioIO.frame, mBytesPerFrame: 0,
                                            mChannelsPerFrame: 1, mBitsPerChannel: 0, mReserved: 0)
        opus = AVAudioFormat(streamDescription: &d)!
    }

    // MARK: host

    func startHosting() {
        stop()
        let s = AVAudioSession.sharedInstance()
        try? s.setCategory(.playAndRecord, mode: .voiceChat, options: [.defaultToSpeaker, .allowBluetooth])
        try? s.setActive(true)
        guard let enc = AVAudioConverter(from: pcm, to: opus) else { return }
        enc.bitRate = Int(core.liveAudioBitrateHint)
        encoder = enc
        let input = engine.inputNode
        let inFmt = input.outputFormat(forBus: 0)
        toPcm = AVAudioConverter(from: inFmt, to: pcm)
        input.installTap(onBus: 0, bufferSize: 1024, format: inFmt) { [weak self] buf, _ in
            self?.q.async { self?.onMic(buf) }
        }
        tapped = true
        try? engine.start()
    }
    private var tapped = false

    private func onMic(_ buf: AVAudioPCMBuffer) {
        guard let conv = toPcm else { return }
        let cap = AVAudioFrameCount(Double(buf.frameLength) * LiveAudioIO.rate / buf.format.sampleRate) + 32
        guard let out = AVAudioPCMBuffer(pcmFormat: pcm, frameCapacity: cap) else { return }
        var fed = false
        conv.convert(to: out, error: nil) { _, status in
            if fed { status.pointee = .noDataNow; return nil }
            fed = true; status.pointee = .haveData; return buf
        }
        if let ch = out.floatChannelData?[0] {
            pendingSamples.append(contentsOf: UnsafeBufferPointer(start: ch, count: Int(out.frameLength)))
        }
        var packets: [Data] = []
        while pendingSamples.count >= Int(Self.frame) {
            let chunk = Array(pendingSamples.prefix(Int(Self.frame)))
            pendingSamples.removeFirst(Int(Self.frame))
            if let p = encode(chunk) { packets.append(p) }
        }
        if !packets.isEmpty, !core.livePushAudio(packets).isEmpty { kick?.kick("") }
    }

    private func encode(_ samples: [Float]) -> Data? {
        guard let enc = encoder, let inBuf = AVAudioPCMBuffer(pcmFormat: pcm, frameCapacity: Self.frame) else { return nil }
        inBuf.frameLength = Self.frame
        samples.withUnsafeBufferPointer { inBuf.floatChannelData![0].update(from: $0.baseAddress!, count: samples.count) }
        let out = AVAudioCompressedBuffer(format: opus, packetCapacity: 1, maximumPacketSize: max(enc.maximumOutputPacketSize, 1500))
        var fed = false
        let st = enc.convert(to: out, error: nil) { _, status in
            if fed { status.pointee = .noDataNow; return nil }
            fed = true; status.pointee = .haveData; return inBuf
        }
        guard st != .error, out.byteLength > 0 else { return nil }
        return Data(bytes: out.data, count: Int(out.byteLength))
    }

    // MARK: listener

    func startListening() {
        stop()
        let s = AVAudioSession.sharedInstance()
        try? s.setCategory(.playback, mode: .spokenAudio)
        try? s.setActive(true)
        decoder = AVAudioConverter(from: opus, to: pcm)
        engine.attach(player)
        engine.connect(player, to: engine.mainMixerNode, format: pcm)
        try? engine.start()
        player.play()
        let t = DispatchSource.makeTimerSource(queue: q)
        t.schedule(deadline: .now(), repeating: .milliseconds(40))
        t.setEventHandler { [weak self] in self?.drain() }
        t.resume()
        timer = t
    }

    private func drain() {
        let now = UInt64(Date().timeIntervalSince1970 * 1000)
        for p in core.liveTakeAudio(nowMs: now) {
            guard let out = p.lost || p.opus.isEmpty ? silence() : decode(p.opus) else { continue }
            player.scheduleBuffer(out, completionHandler: nil)
        }
    }

    private func silence() -> AVAudioPCMBuffer? {
        let b = AVAudioPCMBuffer(pcmFormat: pcm, frameCapacity: Self.frame)
        b?.frameLength = Self.frame // zero-filled
        return b
    }

    private func decode(_ data: Data) -> AVAudioPCMBuffer? {
        guard let dec = decoder, let out = AVAudioPCMBuffer(pcmFormat: pcm, frameCapacity: Self.frame * 3) else { return nil }
        let inBuf = AVAudioCompressedBuffer(format: opus, packetCapacity: 1, maximumPacketSize: data.count)
        data.withUnsafeBytes { inBuf.data.copyMemory(from: $0.baseAddress!, byteCount: data.count) }
        inBuf.byteLength = UInt32(data.count)
        inBuf.packetCount = 1
        inBuf.packetDescriptions?[0] = AudioStreamPacketDescription(mStartOffset: 0, mVariableFramesInPacket: 0, mDataByteSize: UInt32(data.count))
        var fed = false
        let st = dec.convert(to: out, error: nil) { _, status in
            if fed { status.pointee = .noDataNow; return nil }
            fed = true; status.pointee = .haveData; return inBuf
        }
        return st == .error || out.frameLength == 0 ? silence() : out
    }

    func stop() {
        timer?.cancel(); timer = nil
        if engine.isRunning { engine.stop() }
        if tapped { engine.inputNode.removeTap(onBus: 0); tapped = false }
        if engine.attachedNodes.contains(player) { player.stop(); engine.detach(player) }
        q.async { self.pendingSamples = [] }
        encoder = nil; decoder = nil; toPcm = nil
        try? AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)
    }
}

private extension OmcpCore {
    /// 12 kbit/s (Rust `live_audio_bitrate`).
    var liveAudioBitrateHint: UInt32 { 12_000 }
}
