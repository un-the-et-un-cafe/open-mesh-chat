package chat.omcp.app

import chat.omcp.app.core.RustOmcpCore
import chat.omcp.app.data.InMemoryAppStore
import chat.omcp.app.data.MeshRepository
import chat.omcp.app.ui.vm.NewsViewModel
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Before
import org.junit.Test
import java.io.File
import java.nio.file.Files

/** News settings (§5.115) persist through the Rust store across an app restart. */
class NewsPersistenceTest {
    @Before
    fun needLib() {
        val dir = System.getProperty("jna.library.path") ?: ""
        assumeTrue("host libomcp_ffi not built", File(dir, "libomcp_ffi.so").exists())
    }

    @Test fun settingsSurviveRestart() {
        val dir = Files.createTempDirectory("news").toFile()
        val db = File(dir, "omcp.db").path
        val store = InMemoryAppStore()
        val core1 = RustOmcpCore()
        val repo1 = MeshRepository(core1, store, db) { 1L }
        repo1.saveIdentity(core1.generateRecoveryPhrase(), "N")
        val vm1 = NewsViewModel(repo1) { null }
        vm1.toggleTopic("tech")
        vm1.toggleLanguage("fr")
        vm1.setCity("Riverside")
        vm1.mute("sport")
        vm1.setEmergencyAlerts(false)

        // Restart: same store + db, fresh core.
        val core2 = RustOmcpCore()
        val repo2 = MeshRepository(core2, store, db) { 2L }
        assertTrue(repo2.restore())
        val st = NewsViewModel(repo2) { null }.state.value
        assertTrue("tech" in st.topics)
        assertTrue("fr" in st.languages)
        assertEquals("Riverside", st.city)
        assertEquals(setOf("sport"), st.muted)
        assertFalse(st.emergencyAlerts)
        dir.deleteRecursively()
    }

    @Test fun fetchRoleIsTheRustFlag() {
        val core = RustOmcpCore()
        assertTrue(core.newsMayFetch(true))
        assertFalse("gateway-only for now", core.newsMayFetch(false))
    }
}
