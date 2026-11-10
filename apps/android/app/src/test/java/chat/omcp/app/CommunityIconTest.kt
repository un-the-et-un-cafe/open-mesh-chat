package chat.omcp.app

import chat.omcp.app.ui.screens.communityIconColorIndex
import chat.omcp.app.ui.screens.communityIconLetter
import chat.omcp.app.ui.screens.communityIdBytes
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Test
import java.security.MessageDigest

/** DESIGN_SPEC §5.156 letter and colour picking. */
class CommunityIconTest {
    @Test fun letterSkipsSpacePunctuationHashAndEmoji() {
        assertEquals("R", communityIconLetter("Riverside Gardeners"))
        assertEquals("R", communityIconLetter("  Riverside"))
        assertEquals("G", communityIconLetter("#Garden"))
        assertEquals("G", communityIconLetter("  #Garden"))
        assertEquals("P", communityIconLetter("🎉 Party"))
        assertEquals("G", communityIconLetter("🇬🇧 Garden"))
        assertEquals("T", communityIconLetter("1️⃣ Team"))
        assertEquals("H", communityIconLetter("!!!Hello"))
        assertEquals("I", communityIconLetter("istanbul"))
        assertEquals("I", communityIconLetter("i"))
        assertEquals("צ", communityIconLetter("צופי שמי הלילה"))
        assertEquals("ש", communityIconLetter("  #🎉  שלום"))
        assertEquals("SS", communityIconLetter("ßeta"))
    }

    @Test fun letterFallsBackToHash() {
        assertEquals("#", communityIconLetter(""))
        assertEquals("#", communityIconLetter("   "))
        assertEquals("#", communityIconLetter("###"))
        assertEquals("#", communityIconLetter("!!!"))
        assertEquals("#", communityIconLetter("🎉🎉"))
        assertEquals("#", communityIconLetter("🇬🇧"))
    }

    @Test fun colorIndexIsSha256OfPrefixAndRawId() {
        // Vectors computed with SHA-256("omcp-comm-icon-v1" || id)[0] % 6.
        assertEquals(4, communityIconColorIndex(ByteArray(32)))
        assertEquals(2, communityIconColorIndex(ByteArray(32) { 1 }))
        assertEquals(4, communityIconColorIndex(ByteArray(32) { 0x42 }))
        val seq = ByteArray(32) { it.toByte() }
        assertEquals(3, communityIconColorIndex(seq))
        assertEquals(3, communityIconColorIndex("000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f"))
        assertEquals(4, communityIconColorIndex("00".repeat(32)))
        assertArrayEquals(ByteArray(32), communityIdBytes(""))
        assertArrayEquals(ByteArray(32), communityIdBytes("zz"))
        val noPrefix = (MessageDigest.getInstance("SHA-256").digest(ByteArray(32))[0].toInt() and 0xFF) % 6
        assertNotEquals(4, noPrefix)
    }
}
