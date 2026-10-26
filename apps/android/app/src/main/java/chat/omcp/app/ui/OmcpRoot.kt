package chat.omcp.app.ui

import android.Manifest
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Chat
import androidx.compose.material.icons.filled.Groups
import androidx.compose.material.icons.filled.LiveTv
import androidx.compose.material.icons.filled.Newspaper
import androidx.compose.material.icons.filled.Person
import androidx.compose.material3.Icon
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.lifecycle.viewmodel.compose.viewModel
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import androidx.navigation.NavGraph.Companion.findStartDestination
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import androidx.navigation.compose.currentBackStackEntryAsState
import androidx.navigation.compose.rememberNavController
import chat.omcp.app.R
import chat.omcp.app.notify.NotificationPolicy
import chat.omcp.app.backup.CloudBackupStore
import chat.omcp.app.data.BackupGate
import chat.omcp.app.data.MeshRepository
import chat.omcp.app.mesh.MeshPermissions
import chat.omcp.app.mesh.MeshService
import chat.omcp.app.ui.screens.BackupSettingsScreen
import chat.omcp.app.ui.screens.StorageSettingsScreen
import chat.omcp.app.ui.screens.ChatThreadScreen
import chat.omcp.app.ui.screens.ChatsListScreen
import chat.omcp.app.ui.screens.CommunitiesHomeScreen
import chat.omcp.app.ui.screens.ContactAddScreen
import chat.omcp.app.ui.screens.GatewaySettingsScreen
import chat.omcp.app.ui.screens.LiveTabScreen
import chat.omcp.app.ui.screens.NewsHomeScreen
import chat.omcp.app.ui.screens.NewsReaderScreen
import chat.omcp.app.ui.vm.NewsViewModel
import chat.omcp.app.ui.screens.MyQrBlock
import chat.omcp.app.ui.screens.MyQrScreen
import chat.omcp.app.ui.screens.OnboardingFlow
import chat.omcp.app.ui.screens.SecurityCentreScreen
import chat.omcp.app.ui.screens.YouScreen
import chat.omcp.app.ui.vm.BackupSettingsViewModel
import chat.omcp.app.ui.vm.ChatThreadViewModel
import chat.omcp.app.ui.vm.ChatsViewModel
import chat.omcp.app.ui.vm.ContactAddViewModel
import chat.omcp.app.ui.vm.GatewayViewModel
import chat.omcp.app.ui.vm.OnboardingStep
import chat.omcp.app.ui.vm.OnboardingViewModel

private object Routes {
    const val Chats = "chats"
    const val Communities = "communities"
    const val Live = "live"
    const val News = "news"
    const val NewsStory = "news/{id}"
    fun newsStory(id: String) = "news/$id"
    const val You = "you"
    const val Thread = "thread/{chatId}"
    const val Security = "security/{chatId}"
    const val Gateway = "gateway"
    const val ContactAdd = "contact-add"
    const val MyQr = "my-qr"
    const val Backup = "backup"
    const val Storage = "storage"
    const val Relay = "relay"

    fun thread(id: String) = "thread/$id"
    fun security(id: String) = "security/$id"
}

@Composable
private inline fun <reified VM : androidx.lifecycle.ViewModel> vm(key: String? = null, crossinline make: () -> VM): VM =
    viewModel(key = key, factory = viewModelFactory { initializer { make() } })

@Composable
fun OmcpRoot(
    repo: MeshRepository,
    cloud: CloudBackupStore,
    secureWindow: (Boolean) -> Unit,
    requestDeviceAuth: ((() -> Unit) -> Unit),
) {
    val ctx = LocalContext.current
    val onboarded by repo.onboarded.collectAsState()
    val contacts by repo.contacts.collectAsState()
    // Re-evaluate the backup gate whenever contacts change or we recompose after onboarding.
    val gateTick = remember { mutableIntStateOf(0) }
    val gate = remember(onboarded, contacts, gateTick.intValue) { repo.backupGate() }

    val perms = rememberLauncherForActivityResult(ActivityResultContracts.RequestMultiplePermissions()) {
        if (MeshPermissions.hasBle(ctx)) MeshService.start(ctx)
    }
    val requestMesh = { perms.launch(MeshPermissions.BLE + Manifest.permission.POST_NOTIFICATIONS) }
    LaunchedEffect(onboarded) { if (onboarded && MeshPermissions.hasBle(ctx)) MeshService.start(ctx) }

    if (!onboarded || gate == BackupGate.Required) {
        // O6: after 24 h without a backup method, the Secure step is mandatory.
        val app = ctx.applicationContext as? chat.omcp.app.OmcpApplication
        val ovm = vm {
            if (app != null) {
                OnboardingViewModel(
                    repo, cloud,
                    deviceHint = android.os.Build.MODEL ?: "Android",
                    restore = app.restore,
                    runner = chat.omcp.app.restore.RestoreWorker.runner(ctx.applicationContext),
                )
            } else {
                OnboardingViewModel(repo, cloud)
            }
        }
        if (onboarded && gate == BackupGate.Required) {
            LaunchedEffect(Unit) { if (ovm.state.value.step == OnboardingStep.Welcome) ovm.getStarted(); ovm.profileNext() }
        }
        OnboardingFlow(
            vm = ovm,
            myQr = {
                val id = repo.core.identity()
                if (id != null) MyQrBlock(repo.core.qrPayload(), id.displayName, id.peerIdHex)
            },
            onFindNearby = requestMesh,
            secureWindow = secureWindow,
        )
        val st by ovm.state.collectAsState()
        LaunchedEffect(st.step) { if (st.step == OnboardingStep.Done) gateTick.intValue++ }
        return
    }

    val nav = rememberNavController()
    val newsVm = vm { NewsViewModel(repo) }
    val newsCtx = androidx.compose.ui.platform.LocalContext.current
    LaunchedEffect(Unit) {
        // Gateway phones fetch Google News RSS when the gateway rules allow it.
        while (true) {
            val cm = newsCtx.getSystemService(android.net.ConnectivityManager::class.java)
            val caps = cm?.getNetworkCapabilities(cm.activeNetwork)
            val type = when {
                caps == null -> chat.omcp.app.core.NetworkType.Offline
                caps.hasTransport(android.net.NetworkCapabilities.TRANSPORT_WIFI) || caps.hasTransport(android.net.NetworkCapabilities.TRANSPORT_ETHERNET) -> chat.omcp.app.core.NetworkType.Wifi
                else -> chat.omcp.app.core.NetworkType.Cellular
            }
            val metered = caps != null && !caps.hasCapability(android.net.NetworkCapabilities.NET_CAPABILITY_NOT_METERED)
            newsVm.fetchNow(type, metered)
            kotlinx.coroutines.delay(5 * 60_000L)
        }
    }
    val backStack by nav.currentBackStackEntryAsState()
    val route = backStack?.destination?.route
    val showBar = route in setOf(Routes.Chats, Routes.News, Routes.Communities, Routes.You)

    Scaffold(
        bottomBar = {
            if (showBar) {
                NavigationBar {
                    NavigationBarItem(route == Routes.Chats, { nav.tab(Routes.Chats) }, { Icon(Icons.Default.Chat, null) }, label = { Text(stringResource(R.string.tab_chats)) })
                    NavigationBarItem(route == Routes.News, { nav.tab(Routes.News) }, { Icon(Icons.Default.Newspaper, null) }, label = { Text(stringResource(R.string.tab_news)) })
                    NavigationBarItem(route == Routes.Communities, { nav.tab(Routes.Communities) }, { Icon(Icons.Default.Groups, null) }, label = { Text(stringResource(R.string.tab_communities)) })
                    NavigationBarItem(route == Routes.You, { nav.tab(Routes.You) }, { Icon(Icons.Default.Person, null) }, label = { Text(stringResource(R.string.tab_you)) })
                }
            }
        },
    ) { padding ->
        NavHost(navController = nav, startDestination = Routes.Chats, modifier = Modifier.padding(padding)) {
            composable(Routes.Chats) {
                ChatsListScreen(
                    vm = vm { ChatsViewModel(repo) },
                    backupGate = gate,
                    onOpenChat = { nav.navigate(Routes.thread(it)) },
                    onAddContact = { nav.navigate(Routes.ContactAdd) },
                    onBackup = { nav.navigate(Routes.Backup) },
                )
            }
            composable(Routes.Communities) { CommunitiesHomeScreen(onOpenLive = { nav.navigate(Routes.Live) }) }
            composable(Routes.News) { entry ->
                NewsHomeScreen(vm = newsVm, onOpen = { nav.navigate(Routes.newsStory(it)) })
            }
            composable(Routes.NewsStory) { entry ->
                NewsReaderScreen(newsVm, entry.arguments?.getString("id").orEmpty(), repo.core.contacts(), onBack = { nav.popBackStack() })
            }
            composable(Routes.Live) { LiveTabScreen() }
            composable(Routes.You) {
                YouScreen(
                    identity = repo.core.identity(),
                    suiteLabel = repo.core.suiteLabel,
                    privateNotifications = repo.appStore.privateNotifications,
                    onSetPrivateNotifications = { repo.appStore.privateNotifications = it },
                    hideOnLockScreen = repo.appStore.hideOnLockScreen,
                    onSetHideOnLockScreen = { repo.appStore.hideOnLockScreen = it },
                    onGateway = { nav.navigate(Routes.Gateway) },
                    onSecurity = { contacts.firstOrNull()?.let { nav.navigate(Routes.security(it.id)) } },
                    onBackup = { nav.navigate(Routes.Backup) },
                    onStorage = { nav.navigate(Routes.Storage) },
                    onRelay = { nav.navigate(Routes.Relay) },
                    onMyQr = { nav.navigate(Routes.MyQr) },
                )
            }
            composable(Routes.Thread) { entry ->
                val id = entry.arguments?.getString("chatId") ?: return@composable
                ChatThreadScreen(
                    vm = vm(key = "thread-$id") { ChatThreadViewModel(repo, id) },
                    onBack = { nav.popBackStack() },
                    onSecurity = { nav.navigate(Routes.security(id)) },
                )
            }
            composable(Routes.Security) { entry ->
                val id = entry.arguments?.getString("chatId")
                SecurityCentreScreen(
                    contact = contacts.firstOrNull { it.id == id },
                    suiteLabel = repo.core.suiteLabel,
                    onSetVerified = { v -> id?.let { repo.setVerified(it, v) } },
                    privateNotifications = id != null && NotificationPolicy.isPrivate(repo.appStore, id),
                    onSetPrivateNotifications = { v -> id?.let { NotificationPolicy.setChatPrivate(repo.appStore, it, v) } },
                    burnOption = id?.let { repo.appStore.burnChats[it] } ?: -1,
                    burnUnreadTtl = id?.let { repo.appStore.burnUnreadTtl[it] } ?: chat.omcp.app.burn.BurnSpec.UNREAD_7D,
                    onSetBurn = { o, t -> id?.let { repo.setChatBurn(it, o, t) } },
                    onBack = { nav.popBackStack() },
                )
            }
            composable(Routes.Gateway) {
                GatewaySettingsScreen(vm = vm { GatewayViewModel(repo) }, onBack = { nav.popBackStack() })
            }
            composable(Routes.ContactAdd) {
                ContactAddScreen(
                    vm = vm { ContactAddViewModel(repo) },
                    onBack = { nav.popBackStack() },
                    onMyQr = { nav.navigate(Routes.MyQr) },
                    onOpenChat = { id ->
                        nav.navigate(Routes.thread(id)) { popUpTo(Routes.Chats) }
                        if (!MeshPermissions.hasBle(ctx)) requestMesh()
                    },
                )
            }
            composable(Routes.MyQr) {
                MyQrScreen(
                    vm = vm { ContactAddViewModel(repo) },
                    onBack = { nav.popBackStack() },
                    onScanTheirs = { nav.navigate(Routes.ContactAdd) { popUpTo(Routes.Chats) } },
                )
            }
            composable(Routes.Relay) {
                chat.omcp.app.ui.screens.MeshRelaySettingsScreen(
                    vm = vm { chat.omcp.app.ui.vm.MeshRelayViewModel(repo.core) },
                    onBack = { nav.popBackStack() },
                )
            }
            composable(Routes.Storage) {
                StorageSettingsScreen(vm = vm { chat.omcp.app.ui.vm.StorageViewModel(repo.core) }, onBack = { nav.popBackStack() })
            }
            composable(Routes.Backup) {
                BackupSettingsScreen(
                    vm = vm { BackupSettingsViewModel(repo, cloud) },
                    requestDeviceAuth = requestDeviceAuth,
                    secureWindow = secureWindow,
                    onBack = { gateTick.intValue++; nav.popBackStack() },
                )
            }
        }
    }
}

private fun androidx.navigation.NavHostController.tab(route: String) {
    navigate(route) {
        popUpTo(graph.findStartDestination().id) { saveState = true }
        launchSingleTop = true
        restoreState = true
    }
}
