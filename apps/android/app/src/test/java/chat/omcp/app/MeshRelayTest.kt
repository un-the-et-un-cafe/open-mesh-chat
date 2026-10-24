package chat.omcp.app

import chat.omcp.app.core.CoreLink
import chat.omcp.app.core.IncomingText
import chat.omcp.app.core.MeshRelayPrefs
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

    @Test
    fun relaySettingsViewModelDefaultsAndToggle() {
        needLib()
        val core = RustOmcpCore().apply { load(generateRecoveryPhrase(), "A") }
        val vm = MeshRelayViewModel(core)
        assertEquals(MeshRelayPrefs(true, 50_000_000L), vm.state.value.prefs)
        vm.setEnabled(false)
        vm.setCap(100_000_000L)
        assertEquals(MeshRelayPrefs(false, 100_000_000L), core.meshSettings())
        assertEquals(500_000_000L, core.meshMaxCapBytes())
        assertEquals(4, core.meshMaxLinks())
        val beacon = core.meshBeacon()
        assertEquals(6, beacon.size)
        assertEquals(0x48.toByte(), beacon[0])
        assertTrue(MeshRelayViewModel.CAP_OPTIONS.contains(50_000_000L))
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
