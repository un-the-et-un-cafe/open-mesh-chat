package chat.omcp.app

import chat.omcp.app.core.CoreLink
import chat.omcp.app.core.IncomingText
import chat.omcp.app.core.RustOmcpCore
import chat.omcp.app.mesh.MeshBeacon
import chat.omcp.app.mesh.MeshDutyCycle
import chat.omcp.app.mesh.ScanClassifier
import chat.omcp.app.ui.vm.MeshRelayViewModel
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Test
import java.io.File
import java.util.UUID

/**
 * Multi-hop mesh relay (PROTOCOL §22) through the real UniFFI bindings: A → B → C where
 * A and C are never connected, plus the relay settings, beacon framing and duty cycle.
 */
class MeshRelayTest {
    private fun needLib() {
        val dir = System.getProperty("jna.library.path") ?: ""
        assumeTrue("host libomcp_ffi not built", File(dir, "libomcp_ffi.so").exists())
    }

    /** FIFO per direction, like a GATT link; returns everything received on either side. */
    private fun pump(a: CoreLink, b: CoreLink, first: List<ByteArray>): List<IncomingText> {
        val got = mutableListOf<IncomingText>()
        var toB = first
        var toA = emptyList<ByteArray>()
        while (toB.isNotEmpty() || toA.isNotEmpty()) {
            val nextA = mutableListOf<ByteArray>()
            toB.forEach { p -> assertTrue(p.size <= 512); b.onPdu(p).also { got += it.received; nextA += it.outgoing } }
            val nextB = mutableListOf<ByteArray>()
            (toA + nextA).forEach { p -> a.onPdu(p).also { got += it.received; nextB += it.outgoing } }
            toA = emptyList(); toB = nextB
        }
        return got
    }

    private fun connect(x: RustOmcpCore, y: RustOmcpCore): Triple<CoreLink, CoreLink, List<IncomingText>> {
        val lx = x.openInitiatorLink(y.identity()!!.peerIdHex)
        val ly = y.openResponderLink()
        val got = pump(lx, ly, lx.start().outgoing).toMutableList()
        assertTrue(lx.isEstablished() && ly.isEstablished())
        repeat(4) { got += pump(lx, ly, lx.flush().outgoing); got += pump(ly, lx, ly.flush().outgoing) }
        assertTrue(lx.handle() != 0L && ly.handle() != 0L)
        return Triple(lx, ly, got)
    }

    private fun friends(x: RustOmcpCore, y: RustOmcpCore) {
        x.addContactFromQr(y.qrPayload()); y.addContactFromQr(x.qrPayload())
    }

    @Test
    fun chainAtoBtoCNeverInRange() {
        needLib()
        val a = RustOmcpCore().apply { load(generateRecoveryPhrase(), "A") }
        val b = RustOmcpCore().apply { load(generateRecoveryPhrase(), "B") }
        val c = RustOmcpCore().apply { load(generateRecoveryPhrase(), "C") }
        friends(a, b); friends(b, c); friends(a, c)
        val cid = c.identity()!!.peerIdHex
        val aid = a.identity()!!.peerIdHex
        // No 1:1 MLS group yet: nothing goes on the mesh (PROTOCOL §22.8(1)).
        a.queueText(cid, "direct only")
        assertEquals(0, a.meshStats().ownPending)
        // First handshake creates the group (and delivers the queue directly).
        val (ac, ca, gotDirect) = connect(a, c)
        assertEquals(listOf("direct only"), gotDirect.map { it.text })
        ac.close(); ca.close()

        // From now on A and C are never in range.
        a.queueText(cid, "via B")
        assertEquals(1, a.meshStats().ownPending)

        val (ab, ba, gotB) = connect(a, b)
        assertTrue("relay never delivers to itself", gotB.isEmpty())
        assertEquals(1, b.meshStats().carried)
        assertEquals(listOf("Sent"), a.meshTakeStatusUpdates().filter { it.contactId == cid }.map { it.status }.distinct())
        ab.close(); ba.close()

        val (bc, cb, gotC) = connect(b, c)
        assertEquals(listOf("via B"), gotC.map { it.text })
        assertEquals(aid, gotC.single().contactId)
        bc.close(); cb.close()

        // C's end-to-end DELIVERY_ACK travels back through B.
        val (ba2, ab2, _) = connect(b, a)
        assertTrue(a.meshTakeStatusUpdates().any { it.contactId == cid && it.status == "Delivered" })
        assertEquals("Delivered", a.storedMessages(cid).last().status)
        ba2.close(); ab2.close()
    }

    /** RL1 + RL2: no relay switch or cap setting; the cap is automatic (500 MB, lowered on low storage). */
    @Test
    fun rl1Rl2RelayAlwaysOnWithAutomaticCap() {
        needLib()
        val core = RustOmcpCore().apply { load(generateRecoveryPhrase(), "A") }
        assertEquals(500_000_000L, core.meshStats().storageCapBytes)
        core.meshSetFreeStorage(5_000_000_000L, 100_000_000_000L) // 5 % free
        assertEquals(250_000_000L, core.meshStats().storageCapBytes)
        core.meshSetFreeStorage(50_000_000_000L, 100_000_000_000L)
        assertEquals(500_000_000L, core.meshStats().storageCapBytes)
        // The removed API stays removed (no toggle / cap setter on the core interface).
        val names = chat.omcp.app.core.OmcpCore::class.java.methods.map { it.name }
        assertFalse(names.any { it == "setMeshSettings" || it == "meshSettings" || it == "meshMaxCapBytes" })
        assertEquals(4, core.meshMaxLinks())
        val beacon = core.meshBeacon()
        assertEquals(6, beacon.size)
        assertEquals(0x48.toByte(), beacon[0])
    }

    /** RL4: Settings shows only "Storage used for relaying: X MB". */
    @Test
    fun rl4SettingsShowOnlyStorageUsed() {
        needLib()
        val a = RustOmcpCore().apply { load(generateRecoveryPhrase(), "A") }
        val b = RustOmcpCore().apply { load(generateRecoveryPhrase(), "B") }
        val c = RustOmcpCore().apply { load(generateRecoveryPhrase(), "C") }
        friends(a, b); friends(b, c); friends(a, c)
        val (ac, ca, _) = connect(a, c); ac.close(); ca.close()
        val vm = MeshRelayViewModel(b)
        assertEquals(0L, vm.state.value.storageUsedBytes)
        a.queueText(c.identity()!!.peerIdHex, "via B")
        val (ab, ba, _) = connect(a, b); ab.close(); ba.close()
        vm.refresh()
        assertTrue(vm.state.value.storageUsedBytes > 0)
        assertEquals(b.meshStats().storageUsedBytes, vm.state.value.storageUsedBytes)
    }

    @Test
    fun rl4StorageUsedFormatsAsMb() {
        assertEquals("0", MeshRelayViewModel.formatMb(0))
        assertEquals("0.3", MeshRelayViewModel.formatMb(330_000))
        assertEquals("9.9", MeshRelayViewModel.formatMb(9_900_000))
        assertEquals("120", MeshRelayViewModel.formatMb(120_400_000))
        assertEquals("500", MeshRelayViewModel.formatMb(500_000_000))
    }

    /** RL4 strings: EN/FR carry exactly the read-only line; the old toggle/cap strings are gone. */
    @Test
    fun rl4RelayStringsReadOnlyEnFr() {
        val res = listOf("src/main/res/values/strings.xml", "src/main/res/values-fr/strings.xml")
            .map { File(it) }.filter { it.exists() }
        assumeTrue("resources not on the test path", res.size == 2)
        val (en, fr) = res.map { it.readText() }
        assertTrue(en.contains("<string name=\"relay_storage_used\">Storage used for relaying: %1\$s MB</string>"))
        assertTrue(fr.contains("<string name=\"relay_storage_used\">Stockage utilisé pour le relais : %1\$s Mo</string>"))
        for (s in listOf(en, fr)) {
            listOf("relay_toggle", "relay_cap_title", "relay_usage", "relay_privacy").forEach {
                assertFalse(it, s.contains("name=\"$it"))
            }
        }
        val screen = File("src/main/java/chat/omcp/app/ui/screens/MeshRelaySettingsScreen.kt").readText()
        assertFalse(screen.contains("Switch(") || screen.contains("FilterChip"))
    }

    @Test
    fun beaconRidesAfterTheDiscoveryAd() {
        val ad = ByteArray(13) { it.toByte() }
        val hello = byteArrayOf(0x48, 0, 1, 2, 3, 4)
        val joined = MeshBeacon.join(ad, hello)
        assertEquals(19, joined.size)
        val (ad2, b2) = MeshBeacon.split(joined)
        assertArrayEquals(ad, ad2)
        assertArrayEquals(hello, b2)
        assertNull(MeshBeacon.split(ad).second)
        val svc = UUID.randomUUID()
        val d = ScanClassifier.classify(emptyList(), mapOf(0xFFFF to joined), null, svc, 0xFFFF, "x")
        assertTrue(d is ScanClassifier.Decision.Dial)
        assertArrayEquals(ad, (d as ScanClassifier.Decision.Dial).advertisement)
    }

    @Test
    fun dutyCycleFollowsBattery() {
        val charging = MeshDutyCycle.profile(10, charging = true, powerSave = false, coreMaxLinks = 4)
        assertTrue(charging.continuous)
        assertEquals(MeshDutyCycle.ScanMode.Balanced, charging.scanMode)
        assertEquals(4, charging.maxLinks)
        val normal = MeshDutyCycle.profile(80, charging = false, powerSave = false, coreMaxLinks = 4)
        assertEquals(MeshDutyCycle.ScanMode.LowPower, normal.scanMode)
        assertTrue(normal.continuous && normal.tickMs > charging.tickMs)
        val low = MeshDutyCycle.profile(10, charging = false, powerSave = false, coreMaxLinks = 4)
        assertFalse(low.continuous)
        assertTrue(low.scanning(0) && !low.scanning(31_000) && low.scanning(300_000))
        assertTrue(low.scanWindowMs >= 30_000) // Android scan-restart throttling
        assertEquals(2, low.maxLinks)
        assertFalse(MeshDutyCycle.profile(90, charging = true, powerSave = true, coreMaxLinks = 4).continuous)
    }
}
