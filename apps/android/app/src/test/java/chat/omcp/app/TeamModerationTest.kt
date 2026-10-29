package chat.omcp.app

import chat.omcp.app.core.RustOmcpCore
import chat.omcp.app.data.InMemoryAppStore
import chat.omcp.app.data.MeshRepository
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Before
import org.junit.Test
import java.io.File
import java.nio.file.Files

/** §23.9 through the real core: an action not signed by the shipped team never applies. */
class TeamModerationTest {
    @Before
    fun needLib() {
        val dir = System.getProperty("jna.library.path") ?: ""
        assumeTrue("host libomcp_ffi not built", File(dir, "libomcp_ffi.so").exists())
    }

    /** TAKEDOWN wire form with no signatures: kind ‖ community ‖ epoch ‖ reason ‖ note_len ‖ n=0. */
    private fun unsignedTakedown(community: String) =
        "01" + community + "0000000000000001" + "01" + "0000" + "00"

    @Test fun unsignedOrGarbageActionsAreIgnored() {
        val dir = Files.createTempDirectory("mod").toFile()
        val core = RustOmcpCore()
        val repo = MeshRepository(core, InMemoryAppStore(), File(dir, "omcp.db").path) { 1L }
        repo.saveIdentity(core.generateRecoveryPhrase(), "N")
        val cid = "42".repeat(32)
        val r = core.moderationPublish(unsignedTakedown(cid))
        assertTrue(r, r == "no_team" || r == "below_threshold")
        assertEquals("malformed", core.moderationPublish("01ff"))
        assertNull(core.moderationRemovedReason(cid))
        assertTrue(core.moderationLog().isEmpty())
        if (r == "no_team") assertFalse(core.moderationHasTeam())
    }
}
