package chat.omcp.app

import chat.omcp.app.core.InviteCandidateUi
import chat.omcp.app.core.OpenMemberUi
import chat.omcp.app.ui.screens.daysLeft
import chat.omcp.app.ui.screens.inviteStatusView
import chat.omcp.app.ui.screens.notAccepting
import org.junit.Assert.assertEquals
import org.junit.Test

/** §5.152: statuses exactly as built (no Delivered); switch-off members shown, not invitable. */
class PrivateInvitesUiTest {
    @Test fun statusesHaveNoDeliveredAndUnknownLooksPending() {
        assertEquals(R.string.inv_status_pending, inviteStatusView("Not delivered yet").label)
        assertEquals(R.string.inv_status_pending, inviteStatusView("Delivered").label)
        assertEquals(R.string.inv_status_accepted, inviteStatusView("Accepted").label)
        assertEquals(R.string.inv_status_declined, inviteStatusView("Declined").label)
        assertEquals(R.string.inv_status_expired, inviteStatusView("Expired").label)
    }

    @Test fun expiresInDays() {
        val day = 86_400_000L
        assertEquals(7, daysLeft(7 * day, 0))
        assertEquals(1, daysLeft(10, 0))
        assertEquals(0, daysLeft(0, 5))
    }

    @Test fun membersWithSwitchOffAreListedSeparately() {
        val members = listOf(OpenMemberUi("a", "member", false), OpenMemberUi("b", "member", false), OpenMemberUi("me", "owner", true))
        val off = notAccepting(members, listOf(InviteCandidateUi("a", null)))
        assertEquals(listOf("b"), off.map { it.key })
    }
}
