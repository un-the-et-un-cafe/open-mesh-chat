package chat.omcp.app

import android.app.Application
import chat.omcp.app.backup.CloudBackupStore
import chat.omcp.app.backup.DriveAppDataBackupStore
import chat.omcp.app.backup.InMemoryCloudBackupStore
import chat.omcp.app.core.RustOmcpCore
import chat.omcp.app.data.MeshRepository
import chat.omcp.app.data.PrefsAppStore

class OmcpApplication : Application() {
    lateinit var repository: MeshRepository
        private set

    /** Consent screens requested by background Drive calls; shown by MainActivity. */
    val driveConsent = chat.omcp.app.backup.DriveConsentBroker()

    /**
     * Google Drive appDataFolder store. Live when the build has GOOGLE_OAUTH_CLIENT_ID; debug
     * builds without one use an in-memory simulation (BuildConfig.SIMULATED_DRIVE_BACKUP).
     */
    val cloudBackup: CloudBackupStore by lazy {
        if (BuildConfig.SIMULATED_DRIVE_BACKUP) {
            InMemoryCloudBackupStore()
        } else {
            val tokens = chat.omcp.app.backup.GoogleDriveTokenProvider(this, driveConsent)
            DriveAppDataBackupStore(BuildConfig.GOOGLE_OAUTH_CLIENT_ID) { interactive ->
                chat.omcp.app.backup.DriveAppDataClient(
                    chat.omcp.app.backup.UrlConnectionTransport(), { tokens.token(interactive) }, tokens::invalidate,
                )
            }
        }
    }

    /** Restore-from-backup job state (§5.86); the job itself runs in [chat.omcp.app.restore.RestoreWorker]. */
    val restore by lazy { chat.omcp.app.restore.RestoreCoordinator(repository) }

    /** Encrypted chat backup blobs, next to the identity backup in appDataFolder. */
    val chatBlobs: chat.omcp.app.backup.ChatBlobStore by lazy {
        if (BuildConfig.SIMULATED_DRIVE_BACKUP) {
            chat.omcp.app.backup.InMemoryChatBlobStore()
        } else {
            val tokens = chat.omcp.app.backup.GoogleDriveTokenProvider(this, driveConsent)
            chat.omcp.app.backup.DriveChatBlobStore(BuildConfig.GOOGLE_OAUTH_CLIENT_ID) { interactive ->
                chat.omcp.app.backup.DriveAppDataClient(
                    chat.omcp.app.backup.UrlConnectionTransport(), { tokens.token(interactive) }, tokens::invalidate,
                )
            }
        }
    }

    val receivedFiles by lazy { chat.omcp.app.files.ReceivedFiles(this) }

    override fun onCreate() {
        super.onCreate()
        chat.omcp.app.backup.DriveAuthErrors.appIdentity = AppIdentity.describe(this)
        registerActivityLifecycleCallbacks(chat.omcp.app.security.ScreenSecurity.Callbacks())
        // Encrypted local store (Rust core); its key is sealed with the Android Keystore.
        repository = MeshRepository(RustOmcpCore(), PrefsAppStore(this), getDatabasePath("omcp-store.db").apply { parentFile?.mkdirs() }.path)
        repository.fileSink = receivedFiles
        repository.chatBackup = chat.omcp.app.backup.ChatBackupSync(
            repository.core, chatBlobs, { p -> runCatching { java.io.File(p).readBytes() }.getOrNull() },
        )
        runCatching { chat.omcp.app.backup.ChatBackupWorker.schedule(this) }
        runCatching { chat.omcp.app.data.NewsFetchWorker.schedule(this) }
        runCatching { repository.restore() }
    }
}
