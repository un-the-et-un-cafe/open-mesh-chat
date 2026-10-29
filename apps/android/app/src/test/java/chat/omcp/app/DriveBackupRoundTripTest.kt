package chat.omcp.app

import chat.omcp.app.backup.ChatBackupSync
import chat.omcp.app.backup.DriveAppDataBackupStore
import chat.omcp.app.backup.DriveAppDataClient
import chat.omcp.app.backup.DriveChatBlobStore
import chat.omcp.app.backup.HttpTransport
import chat.omcp.app.core.RustOmcpCore
import chat.omcp.app.data.ChatMessage
import chat.omcp.app.data.InMemoryAppStore
import chat.omcp.app.data.MeshRepository
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Test
import java.io.File
import java.net.URLDecoder

/**
 * A multi-file in-memory fake of the Drive v3 REST API (appDataFolder only), so the real
 * [DriveAppDataClient] / [DriveChatBlobStore] / [DriveAppDataBackupStore] code runs unchanged:
 * name queries, `name contains` listing with paging, multipart create, PATCH overwrite,
 * `alt=media` download and delete.
 */
class FakeDriveRest(private val pageSize: Int = 2) : HttpTransport {
    val files = linkedMapOf<String, Pair<String, ByteArray>>() // id -> (name, bytes)
    val tokensSeen = mutableSetOf<String?>()
    private var nextId = 1

    override fun request(method: String, url: String, headers: Map<String, String>, body: ByteArray?): HttpTransport.Response {
        tokensSeen += headers["Authorization"]
        val m = headers["X-HTTP-Method-Override"] ?: method
        val path = url.substringBefore('?')
        val params = url.substringAfter('?', "").split('&').filter { it.contains('=') }
            .associate { it.substringBefore('=') to URLDecoder.decode(it.substringAfter('='), "UTF-8") }
        return when {
            m == "GET" && params["spaces"] == "appDataFolder" -> {
                assertTrue("only appDataFolder", url.contains("spaces=appDataFolder"))
                val q = params["q"].orEmpty()
                val eq = Regex("name = '([^']+)'").find(q)?.groupValues?.get(1)
                val pre = Regex("name contains '([^']+)'").find(q)?.groupValues?.get(1)
                val all = files.entries.filter { (_, f) -> (eq == null || f.first == eq) && (pre == null || f.first.contains(pre)) }
                val start = params["pageToken"]?.toInt() ?: 0
                val page = all.drop(start).take(pageSize)
                val next = if (start + pageSize < all.size) ""","nextPageToken":"${start + pageSize}"""" else ""
                ok("""{"files":[${page.joinToString(",") { """{"id":"${it.key}","name":"${it.value.first}"}""" }}]$next}""")
            }
            m == "POST" && params["uploadType"] == "multipart" -> {
                val text = String(body!!, Charsets.ISO_8859_1)
                assertTrue(text.contains(""""parents":["appDataFolder"]"""))
                val name = Regex("\"name\":\"([^\"]+)\"").find(text)!!.groupValues[1]
                val blob = text.substringAfter("application/octet-stream\r\n\r\n").substringBeforeLast("\r\n--").toByteArray(Charsets.ISO_8859_1)
                val id = "f${nextId++}"
                files[id] = name to blob
                ok("""{"id":"$id"}""")
            }
            m == "PATCH" && params["uploadType"] == "media" -> {
                val id = path.substringAfterLast('/')
                files[id] = files[id]!!.first to body!!.copyOf()
                ok("""{"id":"$id"}""")
            }
            m == "GET" && params["alt"] == "media" -> files[path.substringAfterLast('/')]?.let { HttpTransport.Response(200, it.second.copyOf()) }
                ?: HttpTransport.Response(404, ByteArray(0))
            m == "DELETE" -> { files.remove(path.substringAfterLast('/')); HttpTransport.Response(204, ByteArray(0)) }
            else -> HttpTransport.Response(500, "unexpected $m $url".toByteArray())
        }
    }

    private fun ok(s: String) = HttpTransport.Response(200, s.toByteArray())
}

class DriveBackupRoundTripTest {
    private fun stores(api: FakeDriveRest): Pair<DriveChatBlobStore, DriveAppDataBackupStore> {
        val client = { _: Boolean -> DriveAppDataClient(api, { "tok" }) }
        return DriveChatBlobStore("configured", client) to DriveAppDataBackupStore("configured", client)
    }

    /** Pure Kotlin: binary blobs survive put/list(paged)/get/overwrite/delete in appDataFolder. */
    @Test fun driveLayerRoundTripsBinaryBlobsWithPaging() {
        val api = FakeDriveRest(pageSize = 2)
        val (chat, ident) = stores(api)
        val blobs = (0 until 5).associate { i -> "hopla-chat-%016d-log.bin".format(i) to ByteArray(65536) { ((it * 31 + i) and 0xFF).toByte() } }
        blobs.forEach { (n, b) -> chat.put(n, b) }
        ident.upload(byteArrayOf(0, 13, 10, 0xFF.toByte()))
        assertEquals(blobs.keys.sorted(), chat.list(ChatBackupSync.CHAT_PREFIX).sorted())
        blobs.forEach { (n, b) -> assertArrayEquals(b, chat.get(n)) }
        assertArrayEquals(byteArrayOf(0, 13, 10, 0xFF.toByte()), ident.download())
        chat.put(blobs.keys.first(), byteArrayOf(7))
        assertArrayEquals(byteArrayOf(7), chat.get(blobs.keys.first()))
        assertEquals(6, api.files.size)
        chat.delete(blobs.keys.last())
        assertNull(chat.get(blobs.keys.last()))
        assertEquals(setOf<String?>("Bearer tok"), api.tokensSeen)
    }

    @Test fun unconfiguredBuildIsUnavailable() {
        val api = FakeDriveRest()
        val client = { _: Boolean -> DriveAppDataClient(api, { "tok" }) }
        assertFalse(DriveChatBlobStore("", client).available)
        assertNull(DriveAppDataBackupStore("", client).download())
    }

    /**
     * Real Rust core + real Kotlin Drive code against the fake REST: back up identity and all chat
     * text to "Drive", wipe, restore on a fresh install, compare every message, Restored state set.
     */
    @Test fun backupToDriveWipeRestoreIsIdentical() {
        val dir = System.getProperty("jna.library.path") ?: ""
        assumeTrue("host libomcp_ffi not built", File(dir, "libomcp_ffi.so").exists())
        val t = System.currentTimeMillis()
        val api = FakeDriveRest(pageSize = 3)
        val (chatStore, identStore) = stores(api)

        val aCore = RustOmcpCore()
        val alice = MeshRepository(aCore, InMemoryAppStore(), null) { t }
        val bCore = RustOmcpCore()
        val bob = MeshRepository(bCore, InMemoryAppStore(), null) { t }
        alice.saveIdentity(aCore.generateRecoveryPhrase(), "Alice")
        bob.saveIdentity(bCore.generateRecoveryPhrase(), "Bob")
        val bobId = alice.addContact(bCore.qrPayload(), verified = true).id
        bob.addContact(aCore.qrPayload(), verified = true)
        for (i in 0 until 8) alice.send(bobId, "message $i · ünïcödé")
        val sync = ChatBackupSync(aCore, chatStore, { null }, { t })
        assertEquals(1, sync.runText(onWifi = true, force = true))
        val pass = "correct horse battery staple"
        identStore.upload(aCore.createBackup(pass, "test", t))
        // Nothing readable in "Drive".
        api.files.values.forEach { (_, b) -> assertFalse(String(b, Charsets.ISO_8859_1).contains("message 1")) }
        api.files.values.filter { it.first.startsWith(ChatBackupSync.CHAT_PREFIX) }.forEach { assertEquals(0, it.second.size % 65536) }
        val before = aCore.storedMessages(bobId).map { Triple(it.id, it.text, it.mine) }

        // Wipe: brand-new core and store, only "Drive" survives.
        val a2Core = RustOmcpCore()
        val alice2 = MeshRepository(a2Core, InMemoryAppStore(), null) { t + 1000 }
        val (chat2, ident2) = stores(api)
        alice2.chatBackup = ChatBackupSync(a2Core, chat2, { null }, { t + 1000 })
        val restored = a2Core.openBackup(ident2.download()!!, pass)
        alice2.restoreFromBackup(restored, "Alice")
        val r = alice2.restoreChats()
        assertNotNull(r)
        assertEquals(before.size, r!!.messages)
        val after = a2Core.storedMessages(bobId).map { Triple(it.id, it.text, it.mine) }
        assertEquals(before, after)
        assertEquals("message 7 · ünïcödé", alice2.messages.value[bobId]?.last { it.status != ChatMessage.Status.System }?.text)
        assertNotNull("Restored indicator", alice2.restoredAtMs.value)
    }
}
