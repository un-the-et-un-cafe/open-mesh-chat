package chat.omcp.app.data

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import chat.omcp.app.core.Contact
import org.json.JSONArray
import org.json.JSONObject
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/**
 * SharedPreferences-backed store. The recovery phrase is sealed with an
 * AES-256-GCM key held in the Android Keystore (StrongBox when available).
 * The local message store's key ([storageKey]) is sealed the same way; the store
 * itself (messages, chats, queue, burn state) is the Rust core's encrypted SQLite file.
 */
class PrefsAppStore(context: Context) : AppStore {
    private val prefs = context.getSharedPreferences("omcp", Context.MODE_PRIVATE)

    override var recoveryPhrase: String?
        get() = prefs.getString(K_PHRASE, null)?.let { runCatching { open(it) }.getOrNull() }
        set(v) {
            prefs.edit().apply { if (v == null) remove(K_PHRASE) else putString(K_PHRASE, seal(v)) }.apply()
        }

    override var storageKey: ByteArray?
        get() = prefs.getString(K_STORAGE_KEY, null)?.let { s -> runCatching { Base64.decode(open(s), Base64.NO_WRAP) }.getOrNull() }
        set(v) {
            prefs.edit().apply {
                if (v == null) remove(K_STORAGE_KEY) else putString(K_STORAGE_KEY, seal(Base64.encodeToString(v, Base64.NO_WRAP)))
            }.commit()
        }

    override var displayName: String?
        get() = prefs.getString("name", null)
        set(v) = prefs.edit().putString("name", v).apply()

    override var contacts: List<Contact>
        get() {
            val arr = JSONArray(prefs.getString("contacts", "[]"))
            return (0 until arr.length()).map {
                val o = arr.getJSONObject(it)
                Contact(o.getString("id"), o.getString("name"), emptyList(), o.optBoolean("verified"))
            }
        }
        set(v) {
            val arr = JSONArray()
            v.forEach { arr.put(JSONObject().put("id", it.id).put("name", it.displayName).put("verified", it.verified)) }
            prefs.edit().putString("contacts", arr.toString()).apply()
        }

    override var privateNotifications: Boolean
        get() = prefs.getBoolean("notif_private", false)
        set(v) = prefs.edit().putBoolean("notif_private", v).apply()

    override var chatPrivateOverride: Map<String, Boolean>
        get() {
            val o = JSONObject(prefs.getString("notif_private_override", "{}")!!)
            return o.keys().asSequence().associateWith { o.getBoolean(it) }
        }
        set(v) {
            val o = JSONObject()
            v.forEach { (a, b) -> o.put(a, b) }
            prefs.edit().putString("notif_private_override", o.toString()).apply()
        }

    override var hideOnLockScreen: Boolean
        get() = prefs.getBoolean("notif_hide_lock", false)
        set(v) = prefs.edit().putBoolean("notif_hide_lock", v).apply()

    override var burnChats: Map<String, Int>
        get() = intMap("burn_chats")
        set(v) = putIntMap("burn_chats", v)

    override var burnUnreadTtl: Map<String, Int>
        get() = intMap("burn_ttl")
        set(v) = putIntMap("burn_ttl", v)

    private fun intMap(k: String): Map<String, Int> {
        val o = JSONObject(prefs.getString(k, "{}")!!)
        return o.keys().asSequence().associateWith { o.getInt(it) }
    }

    private fun putIntMap(k: String, v: Map<String, Int>) {
        val o = JSONObject()
        v.forEach { (a, b) -> o.put(a, b) }
        prefs.edit().putString(k, o.toString()).apply()
    }

    override var gatewayEnabled: Boolean
        get() = prefs.getBoolean("gw_on", false)
        set(v) = prefs.edit().putBoolean("gw_on", v).apply()

    override var gatewayAllowMobile: Boolean
        get() = prefs.getBoolean("gw_mobile", false)
        set(v) = prefs.edit().putBoolean("gw_mobile", v).apply()

    override var backupMethods: Set<String>
        get() = prefs.getStringSet("backup_methods", emptySet())!!.toSet()
        set(v) = prefs.edit().putStringSet("backup_methods", v).apply()

    override var newsLastFetchMs: Long
        get() = prefs.getLong("news_last_fetch", 0)
        set(v) = prefs.edit().putLong("news_last_fetch", v).apply()

    override var lastCloudBackupMs: Long
        get() = prefs.getLong("backup_last", 0)
        set(v) = prefs.edit().putLong("backup_last", v).apply()

    override var backupRemindDeadlineMs: Long
        get() = prefs.getLong("backup_deadline", 0)
        set(v) = prefs.edit().putLong("backup_deadline", v).apply()

    // commit(): the lockout must survive the process being killed right after a wrong try.
    override var restoreFailures: Int
        get() = prefs.getInt("restore_failures", 0)
        set(v) { prefs.edit().putInt("restore_failures", v).commit() }

    override var restoreRound: Int
        get() = prefs.getInt("restore_round", 0)
        set(v) { prefs.edit().putInt("restore_round", v).commit() }

    override var restoreLockedUntilMs: Long
        get() = prefs.getLong("restore_locked_until", 0)
        set(v) { prefs.edit().putLong("restore_locked_until", v).commit() }

    private fun key(): SecretKey {
        val ks = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        (ks.getKey(ALIAS, null) as? SecretKey)?.let { return it }
        fun gen(strongBox: Boolean): SecretKey {
            val kg = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore")
            kg.init(
                KeyGenParameterSpec.Builder(ALIAS, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
                    .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                    .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                    .setKeySize(256)
                    .setIsStrongBoxBacked(strongBox)
                    .build(),
            )
            return kg.generateKey()
        }
        return runCatching { gen(true) }.getOrElse { gen(false) }
    }

    private fun seal(plain: String): String {
        val c = Cipher.getInstance("AES/GCM/NoPadding").apply { init(Cipher.ENCRYPT_MODE, key()) }
        val ct = c.doFinal(plain.toByteArray())
        return Base64.encodeToString(c.iv + ct, Base64.NO_WRAP)
    }

    private fun open(sealed: String): String {
        val raw = Base64.decode(sealed, Base64.NO_WRAP)
        val c = Cipher.getInstance("AES/GCM/NoPadding")
        c.init(Cipher.DECRYPT_MODE, key(), GCMParameterSpec(128, raw, 0, 12))
        return String(c.doFinal(raw, 12, raw.size - 12))
    }

    private companion object {
        const val K_PHRASE = "phrase_sealed"
        const val K_STORAGE_KEY = "storage_key_sealed"
        const val ALIAS = "omcp-identity-wrap"
    }
}
