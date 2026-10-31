package chat.omcp.app

import chat.omcp.app.ui.components.Pseudonym
import org.junit.Assert.assertEquals
import org.junit.Test

/** §5.138 vectors computed independently (Python hashlib). */
class PseudonymTest {
    private fun bits(id: Pseudonym.Id) = id.cells.joinToString("") { if (it) "1" else "0" }

    @Test fun deterministicVectors() {
        val z = Pseudonym.of("00".repeat(32))
        assertEquals("Mellow Turtle", z.name)
        assertEquals(Pseudonym.COLOURS[4], z.colour)
        assertEquals("1111101010000000111001110", bits(z))
        assertEquals("omcp:000000", z.shortKey)
        val a = Pseudonym.of("ab".repeat(32))
        assertEquals("Plucky Puffin", a.name)
        assertEquals(Pseudonym.COLOURS[0], a.colour)
        assertEquals("1101111111100010111011111", bits(a))
        assertEquals(a, Pseudonym.of("ab".repeat(32)))
    }

    @Test fun listsAreFixedSize() {
        assertEquals(32, Pseudonym.ADJ.size)
        assertEquals(32, Pseudonym.NOUN.size)
        assertEquals(32, Pseudonym.ADJ.toSet().size)
        assertEquals(32, Pseudonym.NOUN.toSet().size)
    }

    @Test fun reportReasonsAreTheFinalSeven() {
        assertEquals(listOf(6, 7, 4, 10, 2, 5, 9), chat.omcp.app.ui.screens.REPORT_REASONS.map { it.second })
    }
}
