package chat.omcp.app

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.luminance
import chat.omcp.app.core.NewsStoryUi
import chat.omcp.app.ui.theme.OmcpDark
import chat.omcp.app.ui.theme.OmcpLight
import chat.omcp.app.ui.theme.ThemeMode
import chat.omcp.app.ui.vm.arrangeNews
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** Design v0.1.21 tokens meet WCAG AA (§2.1, §5.102, §5.104) and News ordering (§5.108). */
class DesignTokensTest {
    private fun ratio(a: Color, b: Color): Double {
        val (l1, l2) = listOf(a.luminance() + 0.05, b.luminance() + 0.05).sortedDescending()
        return (l1 / l2).toDouble()
    }

    @Test fun lightTokensAreV0121AndPassAa() {
        assertEquals(Color(0xFF3B495A), OmcpLight.Ink2)
        assertEquals(Color(0xFF73849A), OmcpLight.Line)
        assertTrue(ratio(OmcpLight.Ink, OmcpLight.Surface) >= 4.5)
        assertTrue(ratio(OmcpLight.Ink2, OmcpLight.Surface) >= 4.5)
        assertTrue(ratio(OmcpLight.Ink3, OmcpLight.Surface) >= 4.5)
        assertTrue(ratio(OmcpLight.Line, OmcpLight.Surface) >= 3.0)
        assertTrue(ratio(OmcpLight.BrandInk, OmcpLight.Brand) >= 4.5)
        assertTrue(ratio(OmcpLight.Danger, OmcpLight.Surface) >= 4.5)
        assertTrue(ratio(OmcpLight.Focus, OmcpLight.Surface) >= 3.0)
    }

    @Test fun darkTokensAreV0121AndPassAa() {
        assertEquals(Color(0xFFB4C0CE), OmcpDark.Ink2)
        assertEquals(Color(0xFF687990), OmcpDark.Line)
        for (bg in listOf(OmcpDark.Bg, OmcpDark.Surface)) {
            assertTrue(ratio(OmcpDark.Ink2, bg) >= 4.5)
            assertTrue(ratio(OmcpDark.Ink3, bg) >= 4.5)
            assertTrue(ratio(OmcpDark.Line, bg) >= 3.0)
        }
        assertTrue(ratio(OmcpDark.BrandInk, OmcpDark.Brand) >= 4.5)
        assertTrue(ratio(OmcpDark.Danger, OmcpDark.Surface) >= 4.5)
    }

    @Test fun themeModeResolves() {
        assertTrue(ThemeMode.System.isDark(true))
        assertTrue(!ThemeMode.Light.isDark(true))
        assertTrue(ThemeMode.Dark.isDark(false))
        assertEquals(ThemeMode.System, ThemeMode.parse(null))
        assertEquals(ThemeMode.Dark, ThemeMode.parse("Dark"))
    }

    private fun story(id: String, topic: String, t: Long, emergency: Boolean = false) =
        NewsStoryUi(id, topic, "t$id", "", "p", "Google News", "https://news.google.com/x", t, null, true, emergency)

    @Test fun newsPinsAlertsThenNewestAndHidesMuted() {
        val out = arrangeNews(
            listOf(story("a", "tech", 1), story("b", "sport", 3), story("c", "emergencies", 0, true), story("d", "tech", 2)),
            muted = setOf("sport"),
        )
        assertEquals(listOf("c", "d", "a"), out.map { it.idHex })
    }
}
