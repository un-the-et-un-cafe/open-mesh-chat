package chat.omcp.app.mesh

import android.annotation.SuppressLint
import android.bluetooth.BluetoothDevice
import android.bluetooth.BluetoothGatt
import android.bluetooth.BluetoothGattCallback
import android.bluetooth.BluetoothGattCharacteristic
import android.bluetooth.BluetoothGattDescriptor
import android.bluetooth.BluetoothGattServer
import android.bluetooth.BluetoothGattServerCallback
import android.bluetooth.BluetoothGattService
import android.bluetooth.BluetoothManager
import android.bluetooth.BluetoothProfile
import android.bluetooth.le.AdvertiseCallback
import android.bluetooth.le.AdvertiseData
import android.bluetooth.le.AdvertiseSettings
import android.bluetooth.le.ScanCallback
import android.bluetooth.le.ScanFilter
import android.bluetooth.le.ScanResult
import android.bluetooth.le.ScanSettings
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.os.BatteryManager
import android.os.Build
import android.os.PowerManager
import android.os.ParcelUuid
import android.util.Log
import chat.omcp.app.core.CoreLink
import chat.omcp.app.core.LinkResult
import chat.omcp.app.core.OmcpGatt
import chat.omcp.app.data.MeshRepository
import java.util.UUID
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.Executors
import java.util.concurrent.ScheduledFuture
import java.util.concurrent.TimeUnit

/**
 * Android BLE GATT transport (PROTOCOL §8.1). Every phone is both:
 * - **peripheral**: advertises the OMCP service + 13-byte discovery ad (manufacturer
 *   data in the scan response), hosts Inbox (write) / Outbox (notify) / Control (read);
 *   acts as Noise **responder**;
 * - **central**: scans, and dials a contact only if our PeerID sorts lower (tie-break
 *   so two phones don't both dial); acts as Noise **initiator**.
 *
 * Opaque PDUs go straight into [CoreLink] (omcp-ffi): all crypto/fragmentation is Rust.
 * GATT ops are serialised per connection (Android allows one outstanding op).
 *
 * **Multi-hop relay (PROTOCOL §22, v0.2 beta).** Rust's relay engine rides on every
 * established link (PDU `0x03`); [LinkResult.relayLinks] names *other* links that now
 * have frames to send (a message carried from A to C through us), which we flush.
 * A periodic tick ([MeshDutyCycle]) sends HELLO beacons, expires the carry store and
 * duty-cycles the scanner by battery state; the 6-byte HELLO also rides in the scan
 * response after the 13-byte ad ([MeshBeacon]). Links are capped at
 * [OmcpCore.meshMaxLinks] (fewer on low battery). The foreground [MeshService] keeps
 * relaying with the screen off.
 *
 * Experimental: untested on real hardware in this environment.
 */
@SuppressLint("MissingPermission") // callers check runtime permissions first (MeshService)
class BleMeshTransport(private val context: Context, private val repo: MeshRepository) {
    private val tag = "OmcpBle"
    private val io = Executors.newSingleThreadScheduledExecutor()
    private val bt = context.getSystemService(BluetoothManager::class.java)
    private val service = UUID.fromString(OmcpGatt.SERVICE)
    private val inbox = UUID.fromString(OmcpGatt.INBOX)
    private val outbox = UUID.fromString(OmcpGatt.OUTBOX)
    private val control = UUID.fromString(OmcpGatt.CONTROL)
    private val cccd = UUID.fromString(OmcpGatt.CCCD)

    private var server: BluetoothGattServer? = null

    /** Peripheral side: per-central responder links + pending notify queue. */
    private val serverLinks = ConcurrentHashMap<String, CoreLink>()
    private val notifyQueues = ConcurrentHashMap<String, ArrayDeque<ByteArray>>()
    private val notifyBusy = ConcurrentHashMap<String, Boolean>()
    private val subscribed = ConcurrentHashMap<String, BluetoothDevice>()
    private val serverMore = ConcurrentHashMap<String, Boolean>()
    private val serverPeer = ConcurrentHashMap<String, String>()

    /** Central side: per-peripheral initiator links. */
    private inner class Client(val address: String, var contactId: String?, var probing: Boolean = false) {
        var gatt: BluetoothGatt? = null
        var link: CoreLink? = null
        /** Map key: contact id once known, else the device address while probing. */
        val key: String get() = contactId ?: address
        val writes = ArrayDeque<ByteArray>()
        var busy = false
        /** Rust has more file chunks for this link: flush again when [writes] drains. */
        var more = false
    }
    private val clients = ConcurrentHashMap<String, Client>() // by contactId (or address while probing)

    @Volatile var running = false
        private set

    private var ticker: ScheduledFuture<*>? = null
    private var profile: MeshDutyCycle.Profile? = null
    private var scanOn = false
    private var advertisedBeacon: ByteArray? = null
    private var lastAdvertiseMs = 0L
    /** Link handles with relay frames waiting (drained iteratively, never recursively). */
    private val relayPending = LinkedHashSet<Long>()
    private var draining = false

    fun start() {
        if (running) return
        val adapter = bt?.adapter ?: return
        if (!adapter.isEnabled) return
        running = true
        startServer()
        startAdvertising()
        startScanning()
        repo.kick = chat.omcp.app.data.MeshKick { id -> io.execute { kick(id) } }
        io.execute { tick() }
    }

    fun stop() {
        running = false
        repo.kick = null
        ticker?.cancel(false)
        ticker = null
        scanOn = false
        runCatching { bt.adapter.bluetoothLeScanner?.stopScan(scanCb) }
        runCatching { bt.adapter.bluetoothLeAdvertiser?.stopAdvertising(advCb) }
        clients.values.forEach { c -> c.link?.close(); runCatching { c.gatt?.close() } }
        clients.clear()
        serverLinks.values.forEach { it.close() }
        serverLinks.clear()
        runCatching { server?.close() }
        server = null
        repo.refresh()
    }

    // ---------------- peripheral ----------------

    private fun startServer() {
        val s = bt.openGattServer(context, serverCb) ?: return
        val svc = BluetoothGattService(service, BluetoothGattService.SERVICE_TYPE_PRIMARY)
        svc.addCharacteristic(
            BluetoothGattCharacteristic(
                inbox,
                BluetoothGattCharacteristic.PROPERTY_WRITE,
                BluetoothGattCharacteristic.PERMISSION_WRITE,
            ),
        )
        svc.addCharacteristic(
            BluetoothGattCharacteristic(
                outbox,
                BluetoothGattCharacteristic.PROPERTY_NOTIFY,
                BluetoothGattCharacteristic.PERMISSION_READ,
            ).apply {
                addDescriptor(
                    BluetoothGattDescriptor(
                        cccd,
                        BluetoothGattDescriptor.PERMISSION_READ or BluetoothGattDescriptor.PERMISSION_WRITE,
                    ),
                )
            },
        )
        svc.addCharacteristic(
            BluetoothGattCharacteristic(
                control,
                BluetoothGattCharacteristic.PROPERTY_READ,
                BluetoothGattCharacteristic.PERMISSION_READ,
            ),
        )
        s.addService(svc)
        server = s
    }

    private val serverCb = object : BluetoothGattServerCallback() {
        override fun onConnectionStateChange(device: BluetoothDevice, status: Int, newState: Int) {
            if (newState == BluetoothProfile.STATE_DISCONNECTED) {
                io.execute {
                    serverLinks.remove(device.address)?.close()
                    notifyQueues.remove(device.address)
                    subscribed.remove(device.address)
                    serverMore.remove(device.address)
                    serverPeer.remove(device.address)?.let(repo::pauseTransfers)
                    repo.refresh()
                }
            }
        }

        override fun onCharacteristicReadRequest(d: BluetoothDevice, id: Int, offset: Int, c: BluetoothGattCharacteristic) {
            val value = if (c.uuid == control) repo.core.advertisement() else ByteArray(0)
            server?.sendResponse(d, id, BluetoothGatt.GATT_SUCCESS, offset, value.copyOfRange(minOf(offset, value.size), value.size))
        }

        override fun onDescriptorWriteRequest(
            d: BluetoothDevice, id: Int, desc: BluetoothGattDescriptor,
            prepared: Boolean, needResp: Boolean, offset: Int, value: ByteArray,
        ) {
            if (desc.uuid == cccd) subscribed[d.address] = d
            if (needResp) server?.sendResponse(d, id, BluetoothGatt.GATT_SUCCESS, 0, null)
        }

        override fun onCharacteristicWriteRequest(
            d: BluetoothDevice, id: Int, c: BluetoothGattCharacteristic,
            prepared: Boolean, needResp: Boolean, offset: Int, value: ByteArray,
        ) {
            if (needResp) server?.sendResponse(d, id, BluetoothGatt.GATT_SUCCESS, 0, null)
            if (c.uuid != inbox) return
            io.execute {
                val link = serverLinks.getOrPut(d.address) { repo.core.openResponderLink() }
                val r = runCatching { link.onPdu(value) }.getOrElse {
                    Log.w(tag, "responder pdu rejected: ${it.message}")
                    serverLinks.remove(d.address)?.close()
                    return@execute
                }
                onServerResult(d, r)
            }
        }

        override fun onNotificationSent(d: BluetoothDevice, status: Int) {
            io.execute {
                notifyBusy[d.address] = false
                pumpNotify(d)
            }
        }
    }

    private fun enqueueNotify(d: BluetoothDevice, pdu: ByteArray) {
        notifyQueues.getOrPut(d.address) { ArrayDeque() }.addLast(pdu)
        pumpNotify(d)
    }

    private fun onServerResult(d: BluetoothDevice, r: LinkResult) {
        r.peerContactId?.let { serverPeer[d.address] = it }
        serverMore[d.address] = r.morePending
        handle(r)
        r.outgoing.forEach { enqueueNotify(d, it) }
        if (r.outgoing.isEmpty()) pumpNotify(d)
    }

    private fun pumpNotify(d: BluetoothDevice) {
        if (notifyBusy[d.address] == true) return
        val q = notifyQueues.getOrPut(d.address) { ArrayDeque() }
        val next = q.removeFirstOrNull() ?: run {
            // Queue drained: pull the next file chunks for this peer, if any.
            val link = serverLinks[d.address]
            if (serverMore[d.address] == true && link != null && link.isEstablished()) {
                serverMore[d.address] = false
                val r = runCatching { link.flush() }.getOrNull() ?: return
                serverMore[d.address] = r.morePending
                handle(r)
                q.addAll(r.outgoing)
            }
            q.removeFirstOrNull() ?: return
        }
        val ch = server?.getService(service)?.getCharacteristic(outbox) ?: return
        notifyBusy[d.address] = true
        if (Build.VERSION.SDK_INT >= 33) {
            server?.notifyCharacteristicChanged(d, ch, false, next)
        } else {
            @Suppress("DEPRECATION")
            ch.value = next
            @Suppress("DEPRECATION")
            server?.notifyCharacteristicChanged(d, ch, false)
        }
    }

    private fun startAdvertising() {
        val adv = bt.adapter.bluetoothLeAdvertiser ?: return
        val settings = AdvertiseSettings.Builder()
            .setAdvertiseMode(AdvertiseSettings.ADVERTISE_MODE_BALANCED)
            .setConnectable(true)
            .build()
        val data = AdvertiseData.Builder().addServiceUuid(ParcelUuid(service)).build()
        // Manufacturer data (13-byte ad) for Android centrals; iOS centrals read it from Control.
        val beacon = runCatching { repo.core.meshBeacon() }.getOrDefault(ByteArray(0))
        advertisedBeacon = beacon
        lastAdvertiseMs = System.currentTimeMillis()
        val scanResp = AdvertiseData.Builder()
            .addManufacturerData(OmcpGatt.MANUFACTURER_ID, MeshBeacon.join(repo.core.advertisement(), beacon))
            .build()
        adv.startAdvertising(settings, data, scanResp, advCb)
    }

    private val advCb = object : AdvertiseCallback() {
        override fun onStartFailure(errorCode: Int) = Log.w(tag, "advertise failed $errorCode").let { }
    }

    // ---------------- central ----------------

    /**
     * Not a strict service-UUID-only filter (iPhones would never match in the background, see
     * [ScanClassifier]). The filters are OR'ed and keep working with the screen off:
     * OMCP service UUID, OMCP manufacturer data (Android peers), OMCP local name (foreground
     * iPhones) and Apple's overflow-area manufacturer data (backgrounded iPhones).
     */
    private fun startScanning() {
        val scanner = bt.adapter.bluetoothLeScanner ?: return
        val filters = listOf(
            ScanFilter.Builder().setServiceUuid(ParcelUuid(service)).build(),
            ScanFilter.Builder().setManufacturerData(OmcpGatt.MANUFACTURER_ID, ByteArray(0)).build(),
            ScanFilter.Builder().setDeviceName(OmcpGatt.LOCAL_NAME).build(),
            ScanFilter.Builder()
                .setManufacturerData(ScanClassifier.APPLE_COMPANY_ID, byteArrayOf(ScanClassifier.APPLE_OVERFLOW_TYPE), byteArrayOf(0xFF.toByte()))
                .build(),
        )
        val mode = when (profile?.scanMode) {
            MeshDutyCycle.ScanMode.LowPower -> ScanSettings.SCAN_MODE_LOW_POWER
            else -> ScanSettings.SCAN_MODE_BALANCED
        }
        val settings = ScanSettings.Builder().setScanMode(mode).build()
        scanner.startScan(filters, settings, scanCb)
        scanOn = true
    }

    private fun stopScanning() {
        runCatching { bt.adapter.bluetoothLeScanner?.stopScan(scanCb) }
        scanOn = false
    }

    // ---------------- relay duty cycle ----------------

    private fun currentProfile(): MeshDutyCycle.Profile {
        val batt = context.registerReceiver(null, IntentFilter(Intent.ACTION_BATTERY_CHANGED))
        val level = batt?.getIntExtra(BatteryManager.EXTRA_LEVEL, -1) ?: -1
        val scale = batt?.getIntExtra(BatteryManager.EXTRA_SCALE, 100) ?: 100
        val pct = if (level >= 0 && scale > 0) level * 100 / scale else 100
        val plugged = (batt?.getIntExtra(BatteryManager.EXTRA_PLUGGED, 0) ?: 0) != 0
        val saver = context.getSystemService(PowerManager::class.java)?.isPowerSaveMode == true
        return MeshDutyCycle.profile(pct, plugged, saver, repo.core.meshMaxLinks())
    }

    /** Periodic: duty-cycle the scanner, run Rust's relay tick, refresh the beacon. */
    private fun tick() {
        if (!running) return
        val p = currentProfile()
        val now = System.currentTimeMillis()
        val modeChanged = profile?.scanMode != p.scanMode
        profile = p
        val want = p.scanning(now)
        if (want && (!scanOn || modeChanged)) {
            if (scanOn) stopScanning()
            startScanning()
        } else if (!want && scanOn) {
            stopScanning()
        }
        queueRelay(runCatching { repo.core.meshTick(now) }.getOrDefault(emptyList()))
        repo.onStatusUpdates(runCatching { repo.core.meshTakeStatusUpdates() }.getOrDefault(emptyList()))
        val beacon = runCatching { repo.core.meshBeacon() }.getOrNull()
        if (beacon != null && !beacon.contentEquals(advertisedBeacon) && now - lastAdvertiseMs >= 60_000) {
            runCatching { bt.adapter.bluetoothLeAdvertiser?.stopAdvertising(advCb) }
            startAdvertising()
        }
        ticker = io.schedule({ tick() }, p.tickMs, TimeUnit.MILLISECONDS)
    }

    private fun linkCount(): Int = clients.size + serverLinks.size

    private fun underLinkLimit(): Boolean = linkCount() < (profile?.maxLinks ?: repo.core.meshMaxLinks())

    private fun queueRelay(handles: List<Long>) {
        relayPending.addAll(handles)
        if (draining) return
        draining = true
        try {
            while (relayPending.isNotEmpty()) {
                val h = relayPending.first()
                relayPending.remove(h)
                flushHandle(h)
            }
        } finally {
            draining = false
        }
    }

    /** Flush the link with Rust handle [h] (client or server side). */
    private fun flushHandle(h: Long) {
        clients.values.firstOrNull { it.link?.handle() == h }?.let { c ->
            val link = c.link ?: return
            if (link.isEstablished()) runCatching { link.flush() }.getOrNull()?.let { onClientResult(c, it) }
            return
        }
        for ((addr, link) in serverLinks) {
            if (link.handle() != h || !link.isEstablished()) continue
            val d = subscribed[addr] ?: return
            runCatching { link.flush() }.getOrNull()?.let { onServerResult(d, it) }
            return
        }
    }

    private val probes = ProbeLimiter()

    private val scanCb = object : ScanCallback() {
        override fun onScanResult(callbackType: Int, result: ScanResult) {
            val rec = result.scanRecord ?: return
            val mfr = HashMap<Int, ByteArray>()
            rec.manufacturerSpecificData?.let { a -> for (i in 0 until a.size()) mfr[a.keyAt(i)] = a.valueAt(i) }
            val decision = ScanClassifier.classify(
                serviceUuids = rec.serviceUuids.orEmpty().map { it.uuid },
                manufacturerData = mfr,
                localName = rec.deviceName,
                omcpService = service,
                omcpManufacturerId = OmcpGatt.MANUFACTURER_ID,
                omcpLocalName = OmcpGatt.LOCAL_NAME,
            )
            when (decision) {
                is ScanClassifier.Decision.Dial -> io.execute { maybeDial(result.device, decision.advertisement) }
                is ScanClassifier.Decision.Probe -> io.execute { maybeProbe(result.device) }
                ScanClassifier.Decision.Ignore -> Unit
            }
        }
    }

    private fun maybeDial(device: BluetoothDevice, adv: ByteArray) {
        if (!running) return
        val contactId = repo.core.contactForAdvertisement(adv) ?: return
        if (clients.containsKey(contactId) || clients.containsKey(device.address)) return
        if (!underLinkLimit()) return
        val me = repo.core.identity()?.peerIdHex ?: return
        if (me >= contactId) return // tie-break: lower PeerID dials
        val c = Client(device.address, contactId)
        clients[contactId] = c
        c.gatt = device.connectGatt(context, false, clientCb(c), BluetoothDevice.TRANSPORT_LE)
    }

    /** iPhone (no manufacturer data): connect, discover services, read Control, then decide. */
    private fun maybeProbe(device: BluetoothDevice) {
        if (!running || clients.containsKey(device.address)) return
        if (clients.values.any { it.address == device.address }) return
        if (!underLinkLimit()) return
        if (!probes.tryBegin(device.address)) return
        val c = Client(device.address, null, probing = true)
        clients[device.address] = c
        c.gatt = device.connectGatt(context, false, clientCb(c), BluetoothDevice.TRANSPORT_LE)
    }

    private fun endProbe(c: Client, useful: Boolean) {
        if (!c.probing) return
        c.probing = false
        probes.end(c.address, useful)
    }

    private fun clientCb(c: Client) = object : BluetoothGattCallback() {
        override fun onConnectionStateChange(g: BluetoothGatt, status: Int, newState: Int) {
            when (newState) {
                BluetoothProfile.STATE_CONNECTED -> g.requestMtu(517)
                BluetoothProfile.STATE_DISCONNECTED -> io.execute {
                    endProbe(c, useful = false)
                    c.link?.close()
                    clients.remove(c.key)
                    c.contactId?.let(repo::pauseTransfers)
                    runCatching { g.close() }
                    repo.refresh()
                }
            }
        }

        override fun onMtuChanged(g: BluetoothGatt, mtu: Int, status: Int) {
            g.discoverServices()
        }

        override fun onServicesDiscovered(g: BluetoothGatt, status: Int) {
            val svc = g.getService(service) ?: return g.disconnect()
            if (c.contactId == null) {
                // Probe: learn who this is from the 13-byte ad on Control.
                val ctl = svc.getCharacteristic(control) ?: return g.disconnect()
                g.readCharacteristic(ctl)
                return
            }
            subscribe(g)
        }

        @Deprecated("API < 33 path")
        override fun onCharacteristicRead(g: BluetoothGatt, ch: BluetoothGattCharacteristic, status: Int) {
            if (Build.VERSION.SDK_INT < 33) {
                @Suppress("DEPRECATION")
                onControl(g, ch, ch.value ?: ByteArray(0), status)
            }
        }

        override fun onCharacteristicRead(g: BluetoothGatt, ch: BluetoothGattCharacteristic, value: ByteArray, status: Int) =
            onControl(g, ch, value, status)

        private fun onControl(g: BluetoothGatt, ch: BluetoothGattCharacteristic, value: ByteArray, status: Int) {
            if (ch.uuid != control) return
            io.execute {
                val cid = if (status == BluetoothGatt.GATT_SUCCESS) repo.core.contactForAdvertisement(value) else null
                val me = repo.core.identity()?.peerIdHex
                // Not a contact, already connected, or they should dial us (tie-break).
                if (cid == null || me == null || me >= cid || clients.containsKey(cid)) {
                    endProbe(c, useful = false)
                    g.disconnect()
                    return@execute
                }
                endProbe(c, useful = true)
                clients.remove(c.key)
                c.contactId = cid
                clients[cid] = c
                subscribe(g)
            }
        }

        private fun subscribe(g: BluetoothGatt) {
            val ch = g.getService(service)?.getCharacteristic(outbox) ?: return g.disconnect()
            g.setCharacteristicNotification(ch, true)
            val d = ch.getDescriptor(cccd) ?: return g.disconnect()
            if (Build.VERSION.SDK_INT >= 33) {
                g.writeDescriptor(d, BluetoothGattDescriptor.ENABLE_NOTIFICATION_VALUE)
            } else {
                @Suppress("DEPRECATION")
                d.value = BluetoothGattDescriptor.ENABLE_NOTIFICATION_VALUE
                @Suppress("DEPRECATION")
                g.writeDescriptor(d)
            }
        }

        override fun onDescriptorWrite(g: BluetoothGatt, d: BluetoothGattDescriptor, status: Int) {
            io.execute {
                val cid = c.contactId ?: return@execute
                val link = runCatching { repo.core.openInitiatorLink(cid) }.getOrNull() ?: return@execute
                c.link = link
                onClientResult(c, link.start())
            }
        }

        override fun onCharacteristicWrite(g: BluetoothGatt, ch: BluetoothGattCharacteristic, status: Int) {
            io.execute {
                c.busy = false
                pumpWrites(c)
            }
        }

        @Deprecated("API < 33 path")
        override fun onCharacteristicChanged(g: BluetoothGatt, ch: BluetoothGattCharacteristic) {
            if (Build.VERSION.SDK_INT < 33) {
                @Suppress("DEPRECATION")
                onNotify(g, ch, ch.value ?: return)
            }
        }

        override fun onCharacteristicChanged(g: BluetoothGatt, ch: BluetoothGattCharacteristic, value: ByteArray) =
            onNotify(g, ch, value)

        private fun onNotify(g: BluetoothGatt, ch: BluetoothGattCharacteristic, value: ByteArray) {
            if (ch.uuid != outbox) return
            io.execute {
                val link = c.link ?: return@execute
                val r = runCatching { link.onPdu(value) }.getOrElse {
                    Log.w(tag, "initiator pdu rejected: ${it.message}")
                    g.disconnect()
                    return@execute
                }
                onClientResult(c, r)
            }
        }
    }

    private fun onClientResult(c: Client, r: LinkResult) {
        c.more = r.morePending
        handle(r)
        r.outgoing.forEach { c.writes.addLast(it) }
        pumpWrites(c)
    }

    private fun pumpWrites(c: Client) {
        if (c.busy) return
        val g = c.gatt ?: return
        val next = c.writes.removeFirstOrNull() ?: run {
            // Write queue drained: pull the next file chunks, if Rust has more.
            val link = c.link
            if (c.more && link != null && link.isEstablished()) {
                c.more = false
                val r = runCatching { link.flush() }.getOrNull() ?: return
                c.more = r.morePending
                handle(r)
                r.outgoing.forEach { c.writes.addLast(it) }
            }
            c.writes.removeFirstOrNull() ?: return
        }
        val ch = g.getService(service)?.getCharacteristic(inbox) ?: return
        c.busy = true
        if (Build.VERSION.SDK_INT >= 33) {
            g.writeCharacteristic(ch, next, BluetoothGattCharacteristic.WRITE_TYPE_DEFAULT)
        } else {
            @Suppress("DEPRECATION")
            ch.value = next
            ch.writeType = BluetoothGattCharacteristic.WRITE_TYPE_DEFAULT
            @Suppress("DEPRECATION")
            g.writeCharacteristic(ch)
        }
    }

    // ---------------- shared ----------------

    private fun handle(r: LinkResult) {
        r.received.forEach { repo.onReceived(it); MeshNotifications.onMessage(context, repo, it) }
        r.opened.forEach(repo::onOpened)
        r.fileProgress.forEach(repo::onFileProgress)
        r.files.forEach(repo::onFileReceived)
        r.fileFailures.forEach(repo::onFileFailed)
        r.restoreNotices.forEach(repo::onRestoreNotice)
        r.mediaRequests.forEach(repo::onMediaRequest)
        r.resendAsks.forEach(repo::onResendAsk)
        r.mediaUpdates.forEach(repo::onMediaUpdate)
        r.peerContactId?.let { repo.markFlushed(it, r.flushed) } ?: repo.refresh()
        if (r.relayLinks.isNotEmpty()) queueRelay(r.relayLinks)
        repo.onStatusUpdates(runCatching { repo.core.meshTakeStatusUpdates() }.getOrDefault(emptyList()))
    }

    /** New text queued for [contactId]: flush on whichever link to them is up. */
    private fun kick(contactId: String) {
        // A copy also went to the mesh relay (multi-hop): push it onto every link that has frames.
        queueRelay(runCatching { repo.core.meshPendingLinks() }.getOrDefault(emptyList()))
        clients[contactId]?.let { c ->
            val link = c.link ?: return@let
            if (!link.isEstablished()) return@let
            onClientResult(c, link.flush())
            return
        }
        // Peripheral side: flushing a link only drains *its* peer's queue, so
        // flushing every established responder link is safe and simple.
        for ((addr, link) in serverLinks) {
            if (!link.isEstablished()) continue
            val d = subscribed[addr] ?: continue
            onServerResult(d, link.flush())
        }
    }
}
