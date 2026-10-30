package chat.omcp.app.ui.screens

import androidx.compose.foundation.layout.padding
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import chat.omcp.app.R
import chat.omcp.app.ui.components.EmptyState

/** Product decision: keep Communities + Live tabs visible with Coming in v0.2 empty states. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun CommunitiesHomeScreen(onOpenLive: () -> Unit = {}) {
    // REQUIREMENTS "Hopla News": Live ("Coming in v0.2") moves to the top of Communities.
    Scaffold(topBar = { TopAppBar(title = { Text(stringResource(R.string.tab_communities)) }) }) { padding ->
        androidx.compose.foundation.layout.Column(Modifier.padding(padding)) {
            androidx.compose.material3.TextButton(onClick = onOpenLive) { Text(stringResource(R.string.news_live_soon)) }
            EmptyState(
                title = stringResource(R.string.coming_v02_title),
                body = stringResource(R.string.communities_empty_body),
            )
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun LiveTabScreen() {
    Scaffold(topBar = { TopAppBar(title = { Text(stringResource(R.string.tab_live)) }) }) { padding ->
        EmptyState(
            title = stringResource(R.string.coming_v02_title),
            body = stringResource(R.string.live_empty_body),
            modifier = Modifier.padding(padding),
        )
    }
}
