package chat.omcp.app.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Verified
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.unit.dp
import chat.omcp.app.core.PathKind
import chat.omcp.app.ui.theme.LocalOmcpColors

data class ChatRowModel(
    val id: String,
    val title: String,
    val preview: String,
    val time: String,
    val initials: String,
    val verified: Boolean,
    val unread: Int = 0,
    val path: PathKind? = null,
    val isGroup: Boolean = false,
    /** RS2: restored on a new device and not re-verified yet (§5.98). */
    val restoredUnverified: Boolean = false,
    /** Waiting missing-media count (§5.93 clock pill). */
    val waitingMedia: Int = 0,
)

@Composable
fun ChatRow(
    model: ChatRowModel,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val c = LocalOmcpColors.current
    Row(
        modifier = modifier
            .fillMaxWidth()
            .heightIn(min = 64.dp)
            .clickable(onClick = onClick)
            .padding(horizontal = 16.dp, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Box(
            modifier = Modifier
                .size(48.dp)
                .clip(if (model.isGroup) RoundedCornerShape(12.dp) else CircleShape)
                .background(c.surface2),
            contentAlignment = Alignment.Center,
        ) {
            Text(model.initials, style = MaterialTheme.typography.titleMedium)
        }
        Column(Modifier.weight(1f)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(model.title, style = MaterialTheme.typography.titleMedium)
                if (model.verified) {
                    VerifiedBadge()
                } else if (model.restoredUnverified) {
                    RestoredPill(compact = true)
                }
            }
            Text(model.preview, color = c.ink2, style = MaterialTheme.typography.bodyMedium, maxLines = 1)
        }
        Column(horizontalAlignment = Alignment.End) {
            Text(model.time, color = c.ink3, style = MaterialTheme.typography.labelSmall)
            when {
                model.unread > 0 -> {
                    Box(
                        Modifier
                            .padding(top = 4.dp)
                            .clip(RoundedCornerShape(999.dp))
                            .background(MaterialTheme.colorScheme.primary)
                            .padding(horizontal = 7.dp, vertical = 2.dp),
                    ) {
                        Text(
                            "${model.unread}",
                            color = MaterialTheme.colorScheme.onPrimary,
                            style = MaterialTheme.typography.labelSmall,
                        )
                    }
                }
                model.path != null && model.path != PathKind.Mesh -> {
                    PathChip(
                        kind = model.path,
                        label = when (model.path) {
                            PathKind.Gateway -> "Gateway"
                            PathKind.Mixed -> "Mixed"
                            PathKind.Queued -> "Queued"
                            PathKind.Mesh -> "Mesh"
                        },
                        micro = true,
                        modifier = Modifier.padding(top = 4.dp),
                    )
                }
            }
        }
    }
}

/** Small verified check badge next to a name (shown only for verified contacts). */
@Composable
fun VerifiedBadge(modifier: Modifier = Modifier) {
    Icon(
        Icons.Default.Verified,
        contentDescription = "Verified contact",
        tint = LocalOmcpColors.current.ok,
        modifier = modifier
            .padding(start = 4.dp)
            .size(16.dp),
    )
}
