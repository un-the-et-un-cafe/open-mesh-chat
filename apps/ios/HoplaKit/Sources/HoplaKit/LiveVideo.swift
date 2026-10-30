import Foundation

/// One 20 ms playout slot (`opus` empty when lost).
public struct LivePlayoutModel: Equatable, Sendable {
    public var index: UInt64
    public var opus: Data
    public var lost: Bool
    public init(index: UInt64, opus: Data, lost: Bool) { self.index = index; self.opus = opus; self.lost = lost }
}

public struct LiveSignalModel: Equatable, Sendable {
    public var peerHex: String
    public var kind: String
    public var body: String
    public init(peerHex: String, kind: String, body: String) { self.peerHex = peerHex; self.kind = kind; self.body = body }
}

/// SFrame base key + KID for the WebRTC frame cryptor (derived in Rust from the stream key).
public struct LiveMediaKeyModel: Equatable, Sendable, CustomStringConvertible {
    public var baseKey: Data
    public var kid: UInt64
    public init(baseKey: Data, kid: UInt64) { self.baseKey = baseKey; self.kid = kid }
    public var description: String { "LiveMediaKeyModel(kid: \(kid))" }
}

public enum VideoState: Equatable, Sendable { case off, connecting, watching, hosting, noDirectPath, noSlot }

/// WebRTC seam (real one in the app: `WebRtcPeers`). Same contract as Android `RtcPeers`.
public protocol RtcPeers: AnyObject {
    func offerTo(_ peer: String)
    func onRemote(_ peer: String, kind: String, body: String)
    func close(_ peer: String)
    func closeAll()
    func setKey(_ key: LiveMediaKeyModel)
}

/// Online video for Live (docs/LIVE.md), a line-for-line port of Android `LiveVideoController`
/// so both platforms build the same tree and fall back the same way.
public final class LiveVideoController {
    public static let host = String(repeating: "0", count: 64)
    public static let parked = "parked"
    public static let hostFanout = 6
    public static let viewerFanout = 4
    public static let connectTimeoutMs: UInt64 = 20_000

    public private(set) var state: VideoState = .off
    public private(set) var children: [String] = []
    private let core: OmcpCore
    private let peers: RtcPeers
    private let timeoutMs: UInt64
    private var isHost = false
    private var since: UInt64 = 0
    private var key: LiveMediaKeyModel?
    private var parent: String?
    private let lock = NSLock()

    public init(core: OmcpCore, peers: RtcPeers, connectTimeoutMs: UInt64 = LiveVideoController.connectTimeoutMs) {
        self.core = core; self.peers = peers; self.timeoutMs = connectTimeoutMs
    }

    private var maxChildren: Int { isHost ? Self.hostFanout : Self.viewerFanout }

    public func startHosting(now: UInt64) {
        stop(); lock.lock(); defer { lock.unlock() }
        isHost = true; state = .hosting; since = now; refreshKey()
    }

    public func startWatching(now: UInt64) {
        stop(); lock.lock(); defer { lock.unlock() }
        isHost = false; state = .connecting; since = now; refreshKey()
        core.liveVideoJoin()
    }

    public func stop() {
        lock.lock(); defer { lock.unlock() }
        guard state != .off else { return }
        peers.closeAll(); children = []; parent = nil; key = nil; state = .off
    }

    private func refreshKey() {
        guard let k = core.liveMediaKey(), k != key else { return }
        key = k; peers.setKey(k)
    }

    public func poll(now: UInt64) {
        lock.lock(); defer { lock.unlock() }
        guard state != .off else { return }
        // No key = the stream ended or was taken down by the team: close every
        // connection, so we neither play nor forward video to our children.
        if core.liveMediaKey() == nil && key != nil {
            peers.closeAll(); children = []; parent = nil; key = nil; state = .off
            return
        }
        refreshKey()
        for s in core.liveTakeSignals() {
            switch s.kind {
            case "assign":
                if !children.contains(s.peerHex) && children.count < maxChildren {
                    children.append(s.peerHex); peers.offerTo(s.peerHex)
                }
            case "offer":
                if !isHost { parent = s.peerHex; peers.onRemote(s.peerHex, kind: s.kind, body: s.body) }
            case "answer", "ice":
                if s.peerHex == parent || children.contains(s.peerHex) { peers.onRemote(s.peerHex, kind: s.kind, body: s.body) }
            case "bye":
                if s.body == Self.parked {
                    fallBack(.noSlot)
                } else if s.peerHex == parent {
                    peers.close(s.peerHex); parent = nil; state = .connecting; since = now
                } else {
                    children.removeAll { $0 == s.peerHex }; peers.close(s.peerHex)
                }
            default: break
            }
        }
        if state == .connecting && now > since && now - since > timeoutMs { fallBack(.noDirectPath) }
    }

    public func onConnected(_ peer: String) {
        lock.lock(); defer { lock.unlock() }
        if peer == parent && state == .connecting { state = .watching }
    }

    public func onFailed(_ peer: String) {
        lock.lock(); defer { lock.unlock() }
        peers.close(peer)
        if peer == parent {
            fallBack(.noDirectPath)
        } else if let i = children.firstIndex(of: peer) {
            children.remove(at: i)
            if isHost { core.liveVideoDrop(peerHex: peer) }
        }
    }

    private func fallBack(_ s: VideoState) {
        // Leave the tree so the host re-parents our children; audio stays on the mesh.
        if !isHost { core.liveSendSignal(peerHex: Self.host, kind: "bye", body: "") }
        peers.closeAll(); children = []; parent = nil; state = s
    }

    /// ICE body over 0x50: "mid\nindex\ncandidate" (same as Android).
    public static func encodeIce(mid: String?, index: Int32, candidate: String) -> String {
        "\(mid ?? "")\n\(index)\n\(candidate)"
    }

    public static func decodeIce(_ body: String) -> (mid: String, index: Int32, candidate: String)? {
        let p = body.split(separator: "\n", maxSplits: 2, omittingEmptySubsequences: false).map(String.init)
        guard p.count == 3, let i = Int32(p[1]) else { return nil }
        return (p[0], i, p[2])
    }

    /// Message for the Live screen (string key), nil = nothing to say.
    public static func messageKey(_ s: VideoState) -> String? {
        switch s {
        case .connecting: return "live_video_connecting"
        case .noDirectPath: return "live_video_no_path"
        case .noSlot: return "live_video_full"
        default: return nil
        }
    }
}
