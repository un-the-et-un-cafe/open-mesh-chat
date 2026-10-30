package chat.omcp.app.ui.components

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.unit.dp
import java.security.MessageDigest

/**
 * Key-derived member identity for open communities (DESIGN_SPEC §5.138).
 * English in every locale; never reorder the lists (append only with a new salt).
 */
object Pseudonym {
    private const val SALT = "omcp-pseudonym-v1"
    val ADJ = listOf("Quiet", "Amber", "Brisk", "Calm", "Clever", "Cosy", "Daring", "Gentle", "Golden", "Happy", "Hidden",
        "Jolly", "Kind", "Lively", "Lucky", "Mellow", "Misty", "Nimble", "Patient", "Plucky", "Rapid", "Rosy", "Sandy",
        "Silver", "Sleepy", "Sunny", "Swift", "Tidy", "Velvet", "Warm", "Wise", "Zesty")
    val NOUN = listOf("Otter", "Badger", "Beaver", "Crane", "Dolphin", "Finch", "Fox", "Gecko", "Heron", "Ibex", "Koala",
        "Lark", "Lynx", "Marten", "Moose", "Newt", "Owl", "Panda", "Puffin", "Quail", "Robin", "Seal", "Sparrow", "Swan",
        "Tapir", "Toucan", "Turtle", "Walrus", "Wren", "Yak", "Zebra", "Kestrel")
    val COLOURS = listOf(0xFF1D6F8A, 0xFF5A4A9E, 0xFF1F6F66, 0xFF3D5A80, 0xFF6D597A)

    data class Id(val name: String, val colour: Long, val cells: List<Boolean>, val shortKey: String)

    fun of(keyHex: String): Id {
        val key = keyHex.chunked(2).mapNotNull { it.toIntOrNull(16)?.toByte() }.toByteArray()
        val h = MessageDigest.getInstance("SHA-256").run { update(SALT.toByteArray()); update(key); digest() }
        fun b(i: Int) = h[i].toInt() and 0xFF
        val bits = (b(3) shl 8) or b(4)
        // 5×5, mirrored: bit r*3+c fills (r,c) and (r,4−c).
        val cells = List(25) { i ->
            val r = i / 5
            val c = i % 5
            val cc = if (c > 2) 4 - c else c
            (bits shr (r * 3 + cc)) and 1 == 1
        }
        return Id(ADJ[b(0) % 32] + " " + NOUN[b(1) % 32], COLOURS[b(2) % 5], cells, "omcp:" + keyHex.take(6))
    }
}

/** Colour circle with the white identicon at 70 % inset. Decorative. */
@Composable
fun Identicon(id: Pseudonym.Id, modifier: Modifier = Modifier) {
    Canvas(modifier.size(40.dp).clearAndSetSemantics { }) {
        drawCircle(Color(id.colour))
        val inner = size.minDimension * 0.7f
        val cell = inner / 5
        val o = (size.minDimension - inner) / 2
        id.cells.forEachIndexed { i, on ->
            if (on) drawRect(Color.White, Offset(o + (i % 5) * cell, o + (i / 5) * cell), Size(cell, cell))
        }
    }
}
