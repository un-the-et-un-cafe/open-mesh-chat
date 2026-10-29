import XCTest
@testable import HoplaKit

final class ICloudChatRecordsTests: XCTestCase {
    func testBlake2b256Vectors() {
        XCTAssertEqual(Blake2b256.hex(Data("abc".utf8)), "bddd813c634239723171ef3fee98579b94964e3bb1cb3e427262c8c068d52319")
        XCTAssertEqual(Blake2b256.hex(Data()), "0e5751c026e543b2e8ab2eb06099daa1d1e5df47778f7787faab45cdf12fe3a8")
        XCTAssertEqual(Blake2b256.hex(Data(repeating: 0, count: 128)).count, 64)
        XCTAssertNotEqual(Blake2b256.hex(Data(repeating: 0, count: 128)), Blake2b256.hex(Data(repeating: 0, count: 129)))
    }

    func testRecordNamesAndTypes() {
        let t: Int64 = 1_792_800_000_000 // 2026-10-24 UTC, ISO week 43
        let seg = ICloudChatRecords.record(coreName: "hopla-chat-0000000000000026-log.bin", blob: Data([1]), createdMs: t)!
        XCTAssertEqual(seg.type, "ChatLogSegment"); XCTAssertEqual(seg.name, "seg-1a"); XCTAssertEqual(seg.seq, 26)
        let snap = ICloudChatRecords.record(coreName: "hopla-chat-0000000000000027-snap.bin", blob: Data([2]), createdMs: t)!
        XCTAssertEqual(snap.type, "ChatSnapshot"); XCTAssertEqual(snap.name, "snap-202643"); XCTAssertEqual(snap.seq, 27)
        let blob = Data("abc".utf8)
        let media = ICloudChatRecords.record(coreName: "hopla-media-00ff.bin", blob: blob, createdMs: t)!
        XCTAssertEqual(media.type, "MediaBlob")
        XCTAssertEqual(media.name, "media-bddd813c634239723171ef3fee98579b94964e3bb1cb3e427262c8c068d52319")
        XCTAssertEqual(ICloudChatRecords.isoWeek(1_767_225_600_000), "202601") // 2026-01-01 is ISO 2026-W01
        XCTAssertEqual(ICloudChatRecords.zone, "HoplaBackupV1")
    }

    func testRecordStoreRoundTripsThroughManifest() throws {
        let backend = InMemoryChatRecordBackend()
        let store = RecordChatBlobStore(backend: backend, clock: { 1_792_800_000_000 })
        try store.put("hopla-chat-0000000000000001-snap.bin", Data([1]))
        try store.put("hopla-chat-0000000000000002-log.bin", Data([2]))
        try store.put("hopla-media-aa.bin", Data([3]))
        XCTAssertEqual(Set(backend.records.keys), ["snap-202643", "seg-2", "media-" + Blake2b256.hex(Data([3])), "manifest"])
        XCTAssertEqual(backend.records["manifest"]?.type, "BackupManifest")
        // A fresh device reads the same records back by core name.
        // Same fixed clock: the snapshot record name depends on the ISO week (test was date-dependent).
        let other = RecordChatBlobStore(backend: backend, clock: { 1_792_800_000_000 })
        XCTAssertEqual(try other.list(prefix: ChatBackupSync.chatPrefix),
                       ["hopla-chat-0000000000000001-snap.bin", "hopla-chat-0000000000000002-log.bin"])
        XCTAssertEqual(try other.get("hopla-media-aa.bin"), Data([3]))
        // A newer snapshot in the same week replaces the old one.
        try other.put("hopla-chat-0000000000000003-snap.bin", Data([4]))
        try other.delete("hopla-chat-0000000000000001-snap.bin")
        XCTAssertEqual(backend.records["snap-202643"]?.blob, Data([4]))
        try other.delete("hopla-chat-0000000000000002-log.bin")
        XCTAssertNil(backend.records["seg-2"])
        XCTAssertEqual(try other.list(prefix: ChatBackupSync.chatPrefix), ["hopla-chat-0000000000000003-snap.bin"])
    }
}
