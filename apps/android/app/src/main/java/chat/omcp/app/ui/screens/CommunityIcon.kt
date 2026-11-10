package chat.omcp.app.ui.screens

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import chat.omcp.app.ui.theme.LocalOmcpColors
import chat.omcp.app.ui.theme.OmcpExtraColors
import java.security.MessageDigest
import java.text.BreakIterator
import java.util.Locale

/** Tile sizes from DESIGN_SPEC §5.156. */
enum class CommunityIconSize(val tileDp: Int, val letterSp: Int, val radiusDp: Int) {
    Card(40, 16, 10),
    Header(32, 13, 8),
    Detail(56, 22, 12),
}

private const val COLOR_PREFIX = "omcp-comm-icon-v1"

/**
 * First remaining grapheme cluster of [name] after skipping spaces, `#`,
 * punctuation and emoji, uppercased with `Locale.ROOT` (§5.156). `#` if none.
 */
fun communityIconLetter(name: String): String {
    val it = BreakIterator.getCharacterInstance(Locale.ROOT)
    it.setText(name)
    var start = it.first()
    var end = it.next()
    while (end != BreakIterator.DONE) {
        val cluster = name.substring(start, end)
        if (!skipIconCluster(cluster)) return cluster.uppercase(Locale.ROOT)
        start = end
        end = it.next()
    }
    return "#"
}

/** SHA-256("omcp-comm-icon-v1" || raw 32-byte community id)[0] % 6 (§5.156). */
fun communityIconColorIndex(communityId: ByteArray): Int {
    val md = MessageDigest.getInstance("SHA-256")
    md.update(COLOR_PREFIX.toByteArray(Charsets.UTF_8))
    md.update(communityId)
    return (md.digest()[0].toInt() and 0xFF) % 6
}

/** Decode a 32-byte community id from hex; any other input becomes 32 zero bytes. */
fun communityIdBytes(hex: String): ByteArray {
    val h = hex.trim()
    if (h.length != 64 || h.any { !it.isDigit() && it !in 'a'..'f' && it !in 'A'..'F' }) {
        return ByteArray(32)
    }
    return ByteArray(32) { i -> h.substring(i * 2, i * 2 + 2).toInt(16).toByte() }
}

fun communityIconColorIndex(communityIdHex: String): Int =
    communityIconColorIndex(communityIdBytes(communityIdHex))

internal fun iconPalette(index: Int, c: OmcpExtraColors, brand: Color, ink: Color): Pair<Color, Color> =
    when (index.mod(6)) {
        0 -> brand to c.brandSoft
        1 -> c.pathGateway to c.pathGatewaySoft
        2 -> c.pathMixed to c.pathMixedSoft
        3 -> c.ok to c.okSoft
        4 -> c.pathQueue to c.pathQueueSoft
        else -> ink to c.surface2
    }

/** Default community letter tile (TM36, §5.156). Decorative: the adjacent name is the accessible name. */
@Composable
fun CommunityIcon(
    name: String,
    communityIdHex: String = "",
    size: CommunityIconSize = CommunityIconSize.Card,
    modifier: Modifier = Modifier,
) {
    val c = LocalOmcpColors.current
    val scheme = MaterialTheme.colorScheme
    val (fg, bg) = iconPalette(communityIconColorIndex(communityIdHex), c, scheme.primary, scheme.onSurface)
    Box(
        modifier
            .size(size.tileDp.dp)
            .background(bg, RoundedCornerShape(size.radiusDp.dp))
            .clearAndSetSemantics { },
        contentAlignment = Alignment.Center,
    ) {
        Text(
            communityIconLetter(name),
            color = fg,
            fontWeight = FontWeight.Bold,
            fontSize = size.letterSp.sp,
            letterSpacing = 0.sp,
            lineHeight = size.letterSp.sp,
        )
    }
}

private fun skipIconCluster(cluster: String): Boolean {
    if (cluster.isEmpty()) return true
    if (isEmojiCluster(cluster)) return true
    if (cluster.all { it == '#' }) return true
    val cps = cluster.codePoints().toArray()
    if (cps.all(::isUnicodeWhitespace)) return true
    val nonMarks = cps.filterNot(::isMark)
    return nonMarks.isNotEmpty() && nonMarks.all(::isPunctuation) || nonMarks.isEmpty()
}

private fun isEmojiCluster(cluster: String): Boolean {
    val cps = cluster.codePoints().toArray()
    if (cps.any { isExtPictographic(it) }) return true
    // A flag is a Regional_Indicator pair; some BreakIterators emit each RI alone.
    if (cps.any(::isRegionalIndicator)) return true
    if (cps.contains(0x200D) || cps.contains(0x20E3)) return true
    return false
}

private fun isRegionalIndicator(cp: Int) = cp in 0x1F1E6..0x1F1FF

private fun isUnicodeWhitespace(cp: Int): Boolean =
    Character.isWhitespace(cp) || Character.isSpaceChar(cp)

private fun isMark(cp: Int): Boolean = when (Character.getType(cp)) {
    Character.NON_SPACING_MARK.toInt(),
    Character.COMBINING_SPACING_MARK.toInt(),
    Character.ENCLOSING_MARK.toInt(),
    -> true
    else -> false
}

private fun isPunctuation(cp: Int): Boolean = when (Character.getType(cp)) {
    Character.CONNECTOR_PUNCTUATION.toInt(),
    Character.DASH_PUNCTUATION.toInt(),
    Character.START_PUNCTUATION.toInt(),
    Character.END_PUNCTUATION.toInt(),
    Character.INITIAL_QUOTE_PUNCTUATION.toInt(),
    Character.FINAL_QUOTE_PUNCTUATION.toInt(),
    Character.OTHER_PUNCTUATION.toInt(),
    -> true
    else -> false
}

/** Unicode 16.0 Extended_Pictographic ranges. */
private val EXT_PICTOGRAPHIC = arrayOf(
        0xA9..0xA9,
        0xAE..0xAE,
        0x203C..0x203C,
        0x2049..0x2049,
        0x2122..0x2122,
        0x2139..0x2139,
        0x2194..0x2199,
        0x21A9..0x21AA,
        0x231A..0x231B,
        0x2328..0x2328,
        0x2388..0x2388,
        0x23CF..0x23CF,
        0x23E9..0x23F3,
        0x23F8..0x23FA,
        0x24C2..0x24C2,
        0x25AA..0x25AB,
        0x25B6..0x25B6,
        0x25C0..0x25C0,
        0x25FB..0x25FE,
        0x2600..0x2605,
        0x2607..0x2612,
        0x2614..0x2685,
        0x2690..0x2705,
        0x2708..0x2712,
        0x2714..0x2714,
        0x2716..0x2716,
        0x271D..0x271D,
        0x2721..0x2721,
        0x2728..0x2728,
        0x2733..0x2734,
        0x2744..0x2744,
        0x2747..0x2747,
        0x274C..0x274C,
        0x274E..0x274E,
        0x2753..0x2755,
        0x2757..0x2757,
        0x2763..0x2767,
        0x2795..0x2797,
        0x27A1..0x27A1,
        0x27B0..0x27B0,
        0x27BF..0x27BF,
        0x2934..0x2935,
        0x2B05..0x2B07,
        0x2B1B..0x2B1C,
        0x2B50..0x2B50,
        0x2B55..0x2B55,
        0x3030..0x3030,
        0x303D..0x303D,
        0x3297..0x3297,
        0x3299..0x3299,
        0x1F000..0x1F0FF,
        0x1F10D..0x1F10F,
        0x1F12F..0x1F12F,
        0x1F16C..0x1F171,
        0x1F17E..0x1F17F,
        0x1F18E..0x1F18E,
        0x1F191..0x1F19A,
        0x1F1AD..0x1F1E5,
        0x1F201..0x1F20F,
        0x1F21A..0x1F21A,
        0x1F22F..0x1F22F,
        0x1F232..0x1F23A,
        0x1F23C..0x1F23F,
        0x1F249..0x1F3FA,
        0x1F400..0x1F53D,
        0x1F546..0x1F64F,
        0x1F680..0x1F6FF,
        0x1F774..0x1F77F,
        0x1F7D5..0x1F7FF,
        0x1F80C..0x1F80F,
        0x1F848..0x1F84F,
        0x1F85A..0x1F85F,
        0x1F888..0x1F88F,
        0x1F8AE..0x1F8FF,
        0x1F90C..0x1F93A,
        0x1F93C..0x1F945,
        0x1F947..0x1FAFF,
        0x1FC00..0x1FFFD
)

private fun isExtPictographic(cp: Int): Boolean {
    var lo = 0
    var hi = EXT_PICTOGRAPHIC.lastIndex
    while (lo <= hi) {
        val mid = (lo + hi) ushr 1
        val r = EXT_PICTOGRAPHIC[mid]
        when {
            cp < r.first -> hi = mid - 1
            cp > r.last -> lo = mid + 1
            else -> return true
        }
    }
    return false
}
