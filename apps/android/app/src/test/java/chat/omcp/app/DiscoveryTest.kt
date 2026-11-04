package chat.omcp.app

import chat.omcp.app.core.ListingSettingsUi
import chat.omcp.app.ui.screens.CATEGORY_LABELS
import chat.omcp.app.ui.screens.categoryLabel
import chat.omcp.app.ui.screens.sizeRange
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test

/** §20.8 / TM27 / TM31 app-side rules. */
class DiscoveryTest {
    @Test fun listedIsOffByDefault() = assertFalse(ListingSettingsUi().listed)

    @Test fun unknownCategoriesShowAsOther() {
        assertEquals(8, CATEGORY_LABELS.size)
        assertEquals(R.string.u_cat_8, categoryLabel(0))
        assertEquals(R.string.u_cat_8, categoryLabel(42))
        assertEquals(R.string.u_cat_1, categoryLabel(1))
    }

    @Test fun headerShowsBucketRange() {
        assertEquals(1 to 10, sizeRange(3))
        assertEquals(51 to 200, sizeRange(120))
        assertEquals(1_001 to 5_000, sizeRange(4_000))
    }
}
