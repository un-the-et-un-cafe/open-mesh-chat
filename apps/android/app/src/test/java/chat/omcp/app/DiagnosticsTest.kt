package chat.omcp.app

import chat.omcp.app.core.NetworkType
import chat.omcp.app.data.NewsNet
import chat.omcp.app.diag.Diag
import chat.omcp.app.mesh.BleMeshTransport
import chat.omcp.app.mesh.MeshBeacon
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class DiagnosticsTest {
    @Test fun legacyAdvertisingPayloadsFitIn31Bytes() {
        // Primary: flags (3) + complete 128-bit service UUID list (2 + 16).
        assertTrue(3 + 2 + 16 <= 31)
        // Scan response: manufacturer AD (len + type + company id 2) + 13-byte ad + 6-byte HELLO.
        val mfr = MeshBeacon.join(ByteArray(13), ByteArray(6))
        assertTrue("scan response ${2 + 2 + mfr.size} B", 2 + 2 + mfr.size <= 31)
    }

    @Test fun largestPduFitsTheMinimumMtu() {
        // Handshake chunk 1 + 2 + 480 and link frame 1 + 2 + 12 + 400 + 16 must fit MTU - 3.
        assertTrue(1 + 2 + 480 <= BleMeshTransport.MIN_MTU - 3)
        assertTrue(1 + 2 + 12 + 400 + 16 <= BleMeshTransport.MIN_MTU - 3)
    }

    @Test fun wifiReportedAsMeteredIsStillWifi() {
        assertEquals(NetworkType.Wifi to true, NewsNet.classify(listOf(NewsNet.Caps(wifi = true, vpn = false, notMetered = false))))
        assertEquals(NetworkType.Wifi to false, NewsNet.classify(listOf(NewsNet.Caps(wifi = true, vpn = false, notMetered = true))))
        assertEquals(NetworkType.Cellular to true, NewsNet.classify(listOf(NewsNet.Caps(wifi = false, vpn = false, notMetered = false))))
    }

    @Test fun vpnOverWifiCountsAsWifi() {
        val nets = listOf(NewsNet.Caps(wifi = false, vpn = true, notMetered = false), NewsNet.Caps(wifi = true, vpn = false, notMetered = true))
        assertEquals(NetworkType.Wifi to false, NewsNet.classify(nets))
        assertEquals(NetworkType.Offline to false, NewsNet.classify(emptyList()))
    }

    @Test fun permissionsListedForAndroid12To15() {
        val p12 = chat.omcp.app.ui.screens.diagPermissions(31)
        assertTrue(p12.containsAll(listOf("android.permission.BLUETOOTH_SCAN", "android.permission.BLUETOOTH_CONNECT", "android.permission.BLUETOOTH_ADVERTISE")))
        assertFalse("android.permission.POST_NOTIFICATIONS" in p12)
        assertTrue("android.permission.POST_NOTIFICATIONS" in chat.omcp.app.ui.screens.diagPermissions(35))
    }

    @Test fun reportKeepsTheLast50LinesAndNoContent() {
        Diag.reset()
        repeat(60) { Diag.log("line $it") }
        Diag.seen("AA:BB:CC:DD:EE:FF", "0123abcd", -60, known = true, now = 1_000)
        Diag.link("0123abcd", "rejected: not a contact (both phones must add each other)")
        Diag.news = Diag.News(0, "wifi", "never")
        val r = Diag.report(listOf("hdr"), now = 3_000)
        assertFalse(r.contains("line 9\n"))
        assertTrue(r.contains("line 59"))
        assertTrue(r.contains("0123abcd rssi=-60 2s ago contact"))
        assertTrue(r.contains("both phones must add each other"))
        assertFalse("no full MAC", r.contains("AA:BB:CC"))
        assertEquals("…EE:FF", Diag.mask("AA:BB:CC:DD:EE:FF"))
    }
}
