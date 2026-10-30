package chat.omcp.app.core

import chat.omcp.app.burn.BurnContent
import chat.omcp.app.burn.BurnSpec
import chat.omcp.app.burn.OpenedInfo
import chat.omcp.ffi.ContactInfo
import chat.omcp.ffi.FfiException
import chat.omcp.ffi.LinkOutput
import chat.omcp.ffi.MeshLink
import chat.omcp.ffi.Network
import chat.omcp.ffi.OmcpNode
import chat.omcp.ffi.PathKind as FfiPathKind

/** Real core: omcp-ffi (Rust) through UniFFI-generated Kotlin bindings. */
class RustOmcpCore : OmcpCore {
    @Volatile private var node: OmcpNode? = null

    private fun n(): OmcpNode = node ?: error("identity not loaded")

    override val linked = true
    override val suiteLabel: String get() = chat.omcp.ffi.suiteLabel()

    override fun generateRecoveryPhrase(): String = chat.omcp.ffi.generateRecoveryPhrase()
    override fun validateRecoveryPhrase(phrase: String) = chat.omcp.ffi.validateRecoveryPhrase(phrase)

    override fun isRecoveryWord(word: String) = chat.omcp.ffi.isRecoveryWord(word)
    override fun suggestRecoveryWords(prefix: String) = chat.omcp.ffi.suggestRecoveryWords(prefix)

    override fun load(phrase: String, displayName: String): Identity {
        node = OmcpNode.fromPhrase(phrase, displayName)
        return identity()!!
    }

    override fun open(phrase: String, displayName: String, dbPath: String, storageKey: ByteArray): Identity {
        node = try {
            OmcpNode.open(phrase, displayName, dbPath, storageKey)
        } catch (e: FfiException.Storage) {
            throw StorageUnavailableException(e.message, e)
        }
        return identity()!!
    }

    override fun generateStorageKey(): ByteArray = chat.omcp.ffi.generateStorageKey()

    override fun storedMessages(contactId: String): List<StoredMessageInfo> =
        n().storedMessages(contactId).map { it.toApp() }

    override fun setReceivedFilePath(transferId: String, path: String?) = n().setReceivedFilePath(transferId, path)

    override fun restoreHistory(history: ByteArray) = n().restoreHistory(history)

    override fun identity(): Identity? = node?.identity()?.let { Identity(it.peerIdHex, it.displayName) }
    override fun qrPayload() = n().qrPayload()
    override fun advertisement(): ByteArray = n().advertisement()

    override fun previewContact(payload: String): ContactPreview = try {
        val p = n().previewContactFromQr(payload)
        ContactPreview(p.contact.toContact(), p.alreadyKnown, p.mismatchWith?.toContact())
    } catch (e: FfiException) {
        throw IllegalArgumentException(e.message, e)
    }

    override fun addContactFromQr(payload: String): Contact = try {
        n().addContactFromQr(payload).toContact()
    } catch (e: FfiException) {
        throw IllegalArgumentException(e.message, e)
    }

    override fun restoreContact(contact: Contact) =
        n().restoreContactWithKey(contact.id, contact.displayName, contact.verified, contact.dhHex)

    override fun contacts() = n().contacts().map { it.toContact() }
    override fun setVerified(contactId: String, verified: Boolean) = n().setVerified(contactId, verified)

    override fun contactForAdvertisement(adv: ByteArray): String? =
        chat.omcp.ffi.parseAdvertisement(adv)?.let { n().contactForPrefix(it) }

    override fun queueText(contactId: String, text: String) = n().queueText(contactId, text)
    override fun queueMessage(contactId: String, text: String, burn: BurnSpec?) =
        n().queueMessage(contactId, text, burn?.toFfi())
    override fun queueMedia(contactId: String, mime: String, bytes: ByteArray, burn: BurnSpec?) =
        n().queueMedia(contactId, mime, bytes, burn?.toFfi())

    override fun openBurn(msgId: String, nowMs: Long): BurnContent? =
        n().openBurn(msgId, nowMs.toULong())?.let { BurnContent(it.text, it.bytes, it.mime, it.burnAtMs?.toLong()) }
    override fun closeBurn(msgId: String) = n().closeBurn(msgId)
    override fun tickBurn(nowMs: Long): List<String> = n().tickBurn(nowMs.toULong())
    override fun burnIsLive(msgId: String) = n().burnIsLive(msgId)
    override fun burnSpecValid(spec: BurnSpec) = chat.omcp.ffi.burnSpecValid(spec.toFfi())
    override fun queueFile(contactId: String, name: String, mime: String, bytes: ByteArray): FileOfferInfo = try {
        n().queueFile(contactId, name, mime, bytes).let {
            FileOfferInfo(it.transferIdHex, it.contactId, it.name, it.mime, it.size.toLong(), it.numChunks.toInt())
        }
    } catch (e: FfiException.FileTooLarge) {
        throw IllegalArgumentException("file too large", e)
    }
    override fun cancelFile(transferId: String) = n().cancelFile(transferId)
    override fun queuedFor(contactId: String) = n().queuedFor(contactId).toInt()

    override fun pathStatus(): MeshStatus {
        val s = n().pathStatus()
        return MeshStatus(
            s.nearbyPeers.toInt(),
            s.queuedCount.toInt(),
            when (s.path) {
                FfiPathKind.MESH -> PathKind.Mesh
                FfiPathKind.GATEWAY -> PathKind.Gateway
                FfiPathKind.MIXED -> PathKind.Mixed
                FfiPathKind.QUEUED -> PathKind.Queued
            },
        )
    }

    override fun openInitiatorLink(contactId: String): CoreLink = RustLink(n().openInitiatorLink(contactId))
    override fun openResponderLink(): CoreLink = RustLink(n().openResponderLink())

    override fun checkBackupPassword(password: String): PasswordCheck =
        chat.omcp.ffi.checkBackupPassword(password).let { PasswordCheck(it.ok, it.score.toInt(), it.tooShort) }

    override fun createBackup(password: String, deviceHint: String, nowMs: Long): ByteArray =
        n().createBackup(password, deviceHint, nowMs.toULong())

    override fun backupInfo(blob: ByteArray): BackupMeta? = try {
        chat.omcp.ffi.backupInfo(blob).let { BackupMeta(it.createdAtMs.toLong(), it.deviceHint, it.needsPassword) }
    } catch (e: FfiException.BackupTooNew) {
        throw BackupTooNewException()
    } catch (e: FfiException) {
        null
    }

    override fun openBackup(blob: ByteArray, password: String): RestoredBackup = try {
        val r = chat.omcp.ffi.openBackupWithPassword(blob, password)
        RestoredBackup(r.phrase, r.contacts.map { Contact(it.contactId, it.displayName, emptyList(), it.verified, dhHex = it.dhHex) }, r.displayName, r.history)
    } catch (e: FfiException.WrongBackupPassword) {
        throw WrongBackupPasswordException()
    } catch (e: FfiException.BackupTooNew) {
        throw BackupTooNewException()
    } catch (e: FfiException) {
        throw BackupCorruptedException(e.message ?: "unreadable backup")
    }

    override fun recoveryPhrase(): String = n().recoveryPhrase()

    override fun chatBackupDue(nowMs: Long, onWifi: Boolean, allowMobile: Boolean) =
        n().chatBackupDue(nowMs.toULong(), onWifi, allowMobile)
    override fun chatBackupNext(nowMs: Long, forceSnapshot: Boolean) =
        n().chatBackupNext(nowMs.toULong(), forceSnapshot)?.toApp()
    override fun chatBackupCommit(seq: Long) = n().chatBackupCommit(seq.toULong())
    override fun chatBackupRestore(blobs: List<ByteArray>): ChatBackupSummaryInfo = try {
        n().chatBackupRestore(blobs).let { ChatBackupSummaryInfo(it.messages.toInt(), it.mediaPlaceholders.toInt()) }
    } catch (e: FfiException.WrongBackupPassword) {
        throw WrongBackupPasswordException()
    }
    override fun mediaBackupSettings() = n().mediaBackupSettings().let {
        MediaBackupPrefs(it.enabled, it.photos, it.videos, it.voice, it.files, it.mobileData)
    }
    override fun setMediaBackupSettings(p: MediaBackupPrefs): List<String> =
        n().setMediaBackupSettings(chat.omcp.ffi.MediaBackupSettings(p.enabled, p.photos, p.videos, p.voice, p.files, p.mobileData))
    override fun mediaBackupDue(nowMs: Long, onWifi: Boolean) = n().mediaBackupDue(nowMs.toULong(), onWifi)
    override fun mediaBackupTodo() = n().mediaBackupTodo().map { it.toApp() }
    override fun sealMediaBackup(hash: String, bytes: ByteArray) = n().sealMediaBackup(hash, bytes).toApp()
    override fun mediaBackupCommit(hash: String, nowMs: Long) = n().mediaBackupCommit(hash, nowMs.toULong())
    override fun mediaPlaceholders() = n().mediaPlaceholders().map { it.toApp() }
    override fun restoreMediaFromBackup(msgId: String, blob: ByteArray): ByteArray = n().restoreMediaFromBackup(msgId, blob)
    override fun requestMedia(msgId: String) = n().requestMedia(msgId)
    override fun askToResend(msgId: String, note: String?) = n().askToResend(msgId, note)
    override fun meshStats() = n().meshStats().let {
        MeshRelayStats(
            it.carried.toInt(), it.carriedBytes.toLong(), it.ownPending.toInt(), it.neighbours.toInt(),
            it.storageUsedBytes.toLong(), it.storageCapBytes.toLong(),
        )
    }
    private fun story(s: chat.omcp.ffi.NewsStory) = NewsStoryUi(
        s.idHex, s.topic, s.title, s.summary, s.publisher, s.sourceName, s.link,
        s.publishedMs.toLong(), s.via, s.fetchedHere, s.emergency,
    )
    override fun newsFeed(tab: String) = node?.newsFeed(tab)?.map(::story) ?: emptyList()
    override fun newsPrefs(): NewsPrefsUi = node?.newsPrefs()?.let(::prefsUi) ?: NewsPrefsUi()
    override fun newsSetPrefs(p: NewsPrefsUi): NewsPrefsUi {
        val nd = node ?: return p
        return prefsUi(nd.newsSetPrefs(chat.omcp.ffi.NewsPrefs(p.topics, p.langs, p.city?.takeIf { it.isNotBlank() }, p.cityFromLocation, p.muted, p.alertsNotify, p.interestsChosen)))
    }
    private fun prefsUi(p: chat.omcp.ffi.NewsPrefs) =
        NewsPrefsUi(p.topics, p.langs, p.city, p.cityFromLocation, p.muted, p.alertsNotify, p.interestsChosen)
    override fun newsMayFetch(gatewayEnabled: Boolean) = chat.omcp.ffi.newsMayFetch(gatewayEnabled)
    override fun liveStart(title: String, video: Boolean, contactIds: List<String>): String? =
        runCatching { node?.liveStart(title, video, contactIds) }.getOrNull()
    override fun liveInvite(contactId: String) { runCatching { node?.liveInvite(contactId) } }
    override fun liveRemoveListener(contactId: String) { runCatching { node?.liveRemoveListener(contactId) } }
    override fun liveStop() { runCatching { node?.liveStop() } }
    override fun livePushAudio(frames: List<ByteArray>): List<Long> =
        runCatching { node?.livePushAudio(frames)?.map { it.toLong() } }.getOrNull() ?: emptyList()
    override fun liveStreams(): List<LiveStreamUi> = node?.liveStreams()?.map {
        LiveStreamUi(it.streamIdHex, it.title, it.hostContactId, it.mine, it.listening, it.video, it.startedMs.toLong(), it.communityIdHex)
    } ?: emptyList()
    override fun liveJoin(streamIdHex: String, hasInternet: Boolean): Boolean =
        runCatching { node?.liveJoin(streamIdHex, hasInternet) != null }.getOrDefault(false)
    override fun liveLeave() { node?.liveLeave() }
    override fun liveTakeAudio(nowMs: Long): List<LivePlayoutUi> = node?.liveTakeAudio(nowMs.toULong())?.map {
        LivePlayoutUi(it.index.toLong(), it.opus, it.lost, it.fecNext)
    } ?: emptyList()
    override fun liveStats(): LiveStatsUi = node?.liveStats()?.let {
        LiveStatsUi(it.played.toLong(), it.lost.toLong(), it.forwarded.toLong(), it.droppedCongested.toLong(), it.listeners.toInt(), it.droppedModeration.toLong())
    } ?: LiveStatsUi()
    override fun liveVideoJoin() { runCatching { node?.liveVideoJoin() } }
    override fun liveSendSignal(peerHex: String, kind: String, body: String): Boolean {
        val k = signalKind(kind) ?: return false
        return runCatching { node?.liveSendSignal(peerHex, k, body) != null }.getOrDefault(false)
    }
    override fun liveTakeSignals(): List<LiveSignalUi> = node?.liveTakeSignals()?.map {
        LiveSignalUi(it.peerHex, it.kind.name.lowercase(), it.body)
    } ?: emptyList()
    override fun liveVideoDrop(peerHex: String) { runCatching { node?.liveVideoDrop(peerHex) } }
    override fun liveMediaKey(): LiveMediaKeyUi? = node?.liveMediaKey()?.let { LiveMediaKeyUi(it.baseKey, it.kid.toLong()) }
    override fun liveStunServers(): List<String> = chat.omcp.ffi.liveStunServers()
    private fun community(c: chat.omcp.ffi.CommunityInfo) =
        CommunityUi(c.communityIdHex, c.name, c.members.toInt(), c.founderIsMe, c.removedReason?.toInt(), c.liveCount.toInt(), c.checking)
    override fun liveStartBlocked(communityId: String): Boolean = node?.liveStartBlocked(communityId) ?: false
    override fun groupSend(groupId: String, text: String): String? =
        runCatching { node?.privateGroupSend(groupId, text) }.getOrNull()
    override fun groupMessages(groupId: String): List<GroupMessageUi> = node?.privateGroupMessages(groupId)?.map {
        GroupMessageUi(it.messageIdHex, it.senderName, it.body, it.atMs.toLong(), it.mine, it.reportable)
    } ?: emptyList()
    override fun groupReport(groupId: String, messageIds: List<String>, reason: Int, note: String?): ByteArray? =
        runCatching { node?.privateGroupReport(groupId, messageIds, reason.toUByte(), note) }.getOrNull()
    private fun open(c: chat.omcp.ffi.OpenCommunityInfo) =
        OpenCommunityUi(c.communityIdHex, c.name, c.members.toInt(), c.isFounder, c.isModerator, c.inviteHex)
    override fun openCommunities(): List<OpenCommunityUi> = node?.openCommunities()?.map(::open) ?: emptyList()
    override fun openCommunityCreate(name: String): OpenCommunityUi? =
        runCatching { node?.openCommunityCreate(name)?.let(::open) }.getOrNull()
    override fun openCommunityJoin(code: String): OpenCommunityUi? =
        runCatching { node?.openCommunityJoin(code.trim())?.let(::open) }.getOrNull()
    override fun openTopics(communityId: String): List<OpenTopicUi> =
        node?.openCommunityTopics(communityId)?.map { OpenTopicUi(it.topicIdHex, it.title) } ?: emptyList()
    override fun openNewTopic(communityId: String, title: String): String? =
        runCatching { node?.openCommunityNewTopic(communityId, title) }.getOrNull()
    override fun openPosts(communityId: String, topicId: String): List<OpenPostUi> =
        node?.openCommunityPosts(communityId, topicId)?.map {
            OpenPostUi(it.postIdHex, it.parentIdHex, it.authorHex, it.createdMs.toLong(), it.body, it.removedReason)
        } ?: emptyList()
    override fun openPost(communityId: String, topicId: String, parentId: String?, body: String): String? =
        runCatching { node?.openCommunityPost(communityId, topicId, parentId, body) }.getOrNull()
    override fun openMembers(communityId: String): List<OpenMemberUi> =
        node?.openCommunityMembers(communityId)?.map { OpenMemberUi(it.keyHex, it.role, it.isMe) } ?: emptyList()
    override fun openMakeModerator(communityId: String, memberKey: String): Boolean =
        runCatching { node?.openCommunityAppointModerator(communityId, memberKey) != null }.getOrDefault(false)
    override fun openRemovePost(communityId: String, postId: String, reason: String): Boolean =
        runCatching { node?.openCommunityRemovePost(communityId, postId, reason) != null }.getOrDefault(false)
    override fun openReportPost(communityId: String, postId: String, reason: Int, note: String?): ByteArray? =
        runCatching { node?.openCommunityReportPost(communityId, postId, reason.toUByte(), note) }.getOrNull()
    override fun communities(): List<CommunityUi> = node?.communities()?.map(::community) ?: emptyList()
    override fun communityCreate(name: String): CommunityUi? =
        runCatching { node?.communityCreate(name)?.let(::community) }.getOrNull()
    override fun communityInvite(communityId: String, contactId: String): Boolean =
        runCatching { node?.communityInvite(communityId, contactId) != null }.getOrDefault(false)
    override fun communityInvites(): List<CommunityInviteUi> = node?.communityInvites()?.map {
        CommunityInviteUi(it.communityIdHex, it.name, it.fromContactId)
    } ?: emptyList()
    override fun communityAccept(communityId: String): Boolean =
        runCatching { node?.communityAccept(communityId) != null }.getOrDefault(false)
    override fun communityDecline(communityId: String) { runCatching { node?.communityDecline(communityId) } }
    override fun communityMembers(communityId: String): List<CommunityMemberUi> = node?.communityMembers(communityId)?.map {
        CommunityMemberUi(it.peerIdHex, it.name, it.isContact, it.isMe)
    } ?: emptyList()
    override fun liveStartCommunity(communityId: String, title: String, video: Boolean): String? =
        runCatching { node?.liveStartCommunity(communityId, title, video) }.getOrNull()
    override fun liveTakeStarted(): List<Pair<String, String>> =
        node?.liveTakeStarted()?.map { it.streamIdHex to it.communityIdHex } ?: emptyList()
    override fun liveTakeEnded(): List<LiveEndedUi> =
        node?.liveTakeEnded()?.map { LiveEndedUi(it.streamIdHex, it.reason, it.mine) } ?: emptyList()
    override fun moderationLiveRemovedReason(streamIdHex: String): Int? =
        node?.moderationLiveRemovedReason(streamIdHex)?.toInt()
    private fun signalKind(k: String) = chat.omcp.ffi.LiveSignalKind.entries.firstOrNull { it.name.equals(k, ignoreCase = true) }
    override fun moderationHasTeam() = node?.moderationHasTeam() ?: false
    override fun moderationLog(): List<TeamLogUi> = node?.moderationLog()?.map {
        TeamLogUi(it.kind, it.communityIdHex, it.reasonCode.toInt(), it.note, it.epoch.toLong(), it.signedBy, it.appliedMs.toLong())
    } ?: emptyList()
    override fun moderationPublish(actionHex: String): String {
        val nd = node ?: return "no_node"
        return try {
            when (nd.moderationPublish(actionHex)) {
                chat.omcp.ffi.TeamActionResult.APPLIED -> "applied"
                chat.omcp.ffi.TeamActionResult.NO_TEAM -> "no_team"
                chat.omcp.ffi.TeamActionResult.MALFORMED -> "malformed"
                chat.omcp.ffi.TeamActionResult.BAD_SIGNATURE -> "bad_signature"
                chat.omcp.ffi.TeamActionResult.NOT_WHITELISTED -> "not_whitelisted"
                chat.omcp.ffi.TeamActionResult.BELOW_THRESHOLD -> "below_threshold"
                chat.omcp.ffi.TeamActionResult.REPLAY -> "replay"
            }
        } catch (e: Exception) { "malformed" }
    }
    override fun moderationRemovedReason(communityIdHex: String): Int? =
        node?.moderationRemovedReason(communityIdHex)?.toInt()
    override fun newsFetchPlan(wifiOnly: Boolean, network: NetworkType, metered: Boolean, nowMs: Long): NewsFetchPlanUi {
        val nd = node ?: return NewsFetchPlanUi()
        val net = when (network) {
            NetworkType.Offline -> chat.omcp.ffi.Network.Offline
            NetworkType.Wifi -> chat.omcp.ffi.Network.Wifi(metered)
            NetworkType.Cellular -> chat.omcp.ffi.Network.Cellular(metered)
        }
        val p = nd.newsFetchPlan(net, wifiOnly, nowMs.toULong())
        return NewsFetchPlanUi(p.feeds, p.meteredWarning, p.blocked, p.usedTodayBytes.toLong(), p.dailyCapBytes.toLong())
    }
    override fun newsIngestRss(feedUrl: String, body: String, nowMs: Long) =
        node?.newsIngestRss(feedUrl, body, nowMs.toULong())?.toInt() ?: 0
    override fun newsTakeAlerts() = node?.newsTakeAlerts()?.map(::story) ?: emptyList()
    override fun newsCardText(story: NewsStoryUi): String {
        val s = node?.newsStory(story.idHex) ?: return super.newsCardText(story)
        return chat.omcp.ffi.newsCardText(s)
    }
    override fun parseNewsCard(text: String) =
        chat.omcp.ffi.parseNewsCard(text)?.let { NewsCardUi(it.idHex, it.publisher, it.link, it.title) }
    override fun meshSetFreeStorage(freeBytes: Long, totalBytes: Long) {
        node?.meshSetFreeStorage(freeBytes.coerceAtLeast(0).toULong(), totalBytes.coerceAtLeast(0).toULong())
    }
    override fun meshTick(nowMs: Long) = node?.meshTick(nowMs.toULong())?.map { it.toLong() } ?: emptyList()
    override fun meshPendingLinks() = node?.meshPendingLinks()?.map { it.toLong() } ?: emptyList()
    override fun meshTakeStatusUpdates() = node?.meshTakeStatusUpdates()?.map {
        MeshStatusUpdate(it.idHex, it.contactId, if (it.status == chat.omcp.ffi.MessageStatus.DELIVERED) "Delivered" else "Sent")
    } ?: emptyList()
    override fun meshBeacon() = node?.meshBeacon() ?: ByteArray(0)
    override fun meshMaxLinks() = chat.omcp.ffi.meshMaxLinks().toInt()
    override fun keptMediaSettings() = n().keptMediaSettings().let { KeptMediaPrefs(it.maxDays.toInt(), it.maxBytes.toLong()) }
    override fun setKeptMediaSettings(p: KeptMediaPrefs, nowMs: Long) =
        n().setKeptMediaSettings(chat.omcp.ffi.KeptMediaSettings(p.maxDays.toUInt(), p.maxBytes.toULong()), nowMs.toULong()).toInt()
    override fun keptMediaUsage() = n().keptMediaUsage().let { KeptMediaUsageInfo(it.count.toInt(), it.bytes.toLong()) }
    override fun evictKeptMedia(nowMs: Long) = n().evictKeptMedia(nowMs.toULong()).toInt()
    override fun answerMediaRequest(contactId: String, msgId: String, bytes: ByteArray?) =
        n().answerMediaRequest(contactId, msgId, bytes)
    override fun markRestored(nowMs: Long) = n().markRestored(nowMs.toULong()).toApp()
    override fun deviceInfo() = node?.deviceInfo()?.toApp()
    override fun verifyContactFromQr(payload: String): Contact = try {
        n().verifyContactFromQr(payload).toContact()
    } catch (e: FfiException.VerificationFailed) {
        throw VerificationFailedException()
    } catch (e: FfiException) {
        throw IllegalArgumentException(e.message, e)
    }

    override fun maxFileBytes() = chat.omcp.ffi.maxFileBytes().toLong()
    override fun gatewayDailyCapBytes() = chat.omcp.ffi.defaultGatewayDailyCapBytes().toLong()

    override fun gatewayDecision(wifiOnly: Boolean, network: NetworkType, metered: Boolean): GatewayDecision {
        val net = when (network) {
            NetworkType.Offline -> Network.Offline
            NetworkType.Wifi -> Network.Wifi(metered)
            NetworkType.Cellular -> Network.Cellular(metered)
        }
        val d = chat.omcp.ffi.gatewayDecision(wifiOnly, net)
        return GatewayDecision(d.bridge, d.meteredWarning)
    }
}

private fun BurnSpec.toFfi() = chat.omcp.ffi.BurnSpec(
    if (mode == BurnSpec.Mode.OnOpen) chat.omcp.ffi.BurnMode.ON_OPEN else chat.omcp.ffi.BurnMode.TIMER,
    secs.toUInt(),
    unreadTtlSecs.toUInt(),
)

private fun chat.omcp.ffi.BurnSpec.toApp() = BurnSpec(
    if (mode == chat.omcp.ffi.BurnMode.ON_OPEN) BurnSpec.Mode.OnOpen else BurnSpec.Mode.Timer,
    secs.toInt(),
    unreadTtl.toInt(),
)

private fun chat.omcp.ffi.StoredMessage.toApp() = StoredMessageInfo(
    id = idHex,
    contactId = contactId,
    text = text,
    mine = mine,
    timeMs = timeMs.toLong(),
    status = when (status) {
        chat.omcp.ffi.MessageStatus.QUEUED -> "Queued"
        chat.omcp.ffi.MessageStatus.SENT -> "Sent"
        chat.omcp.ffi.MessageStatus.DELIVERED -> "Delivered"
        chat.omcp.ffi.MessageStatus.RECEIVED -> "Received"
        chat.omcp.ffi.MessageStatus.SYSTEM -> "System"
    },
    mime = mime,
    burn = burn?.toApp(),
    burnState = when (burnState) {
        chat.omcp.ffi.StoredBurnState.NONE -> "None"
        chat.omcp.ffi.StoredBurnState.UNOPENED -> "Unopened"
        chat.omcp.ffi.StoredBurnState.OPEN -> "Open"
        chat.omcp.ffi.StoredBurnState.OPENED -> "Opened"
    },
    burnAtMs = burnAtMs?.toLong(),
    file = file?.let {
        StoredFileInfo(
            it.name, it.mime, it.size.toLong(),
            when (it.state) {
                chat.omcp.ffi.StoredFileState.QUEUED -> "Queued"
                chat.omcp.ffi.StoredFileState.SENDING -> "Sending"
                chat.omcp.ffi.StoredFileState.RECEIVING -> "Receiving"
                chat.omcp.ffi.StoredFileState.PAUSED -> "Paused"
                chat.omcp.ffi.StoredFileState.SENT -> "Sent"
                chat.omcp.ffi.StoredFileState.RECEIVED -> "Received"
                chat.omcp.ffi.StoredFileState.FAILED -> "Failed"
                chat.omcp.ffi.StoredFileState.INTEGRITY_FAILED -> "IntegrityFailed"
                chat.omcp.ffi.StoredFileState.MISSING -> "Missing"
                chat.omcp.ffi.StoredFileState.REQUESTED -> "Requested"
                chat.omcp.ffi.StoredFileState.UNAVAILABLE -> "Unavailable"
            },
            it.doneBytes.toLong(), it.localPath,
        )
    },
)

private fun ContactInfo.toContact() =
    Contact(contactId, displayName, safetyNumber, verified, restoredBadge, restoredAtMs?.toLong(), restoredUnverified)

private fun chat.omcp.ffi.ChatBackupBlob.toApp() =
    ChatBackupBlobInfo(name, kind.toInt(), seq.toLong(), blob, deleteBelowSeq?.toLong())

private fun chat.omcp.ffi.MediaBackupItem.toApp() = MediaBackupItemInfo(msgIdHex, hashHex, mime, localPath, blobName)

private fun chat.omcp.ffi.DeviceInfo.toApp() = DeviceKeyInfo(deviceKeyHex, fingerprint, restoredAtMs?.toLong())

private fun chat.omcp.ffi.MediaRequestEvent.toApp() = MediaRequestInfo(contactId, msgIdHex, hashHex, name, mime, localPath, note)

private fun chat.omcp.ffi.StoredFileState.label() = when (this) {
    chat.omcp.ffi.StoredFileState.MISSING -> "Missing"
    chat.omcp.ffi.StoredFileState.REQUESTED -> "Requested"
    chat.omcp.ffi.StoredFileState.UNAVAILABLE -> "Unavailable"
    chat.omcp.ffi.StoredFileState.RECEIVED -> "Received"
    chat.omcp.ffi.StoredFileState.RECEIVING -> "Receiving"
    else -> "Failed"
}

private fun LinkOutput.toResult() = LinkResult(
    outgoing = outgoing,
    received = received.map {
        IncomingText(it.contactId, it.text, it.createdMs.toLong(), it.envelopeIdHex, it.burn?.toApp(), it.mime)
    },
    established = established,
    peerContactId = peerContactId,
    flushed = flushed.toInt(),
    opened = opened.map { OpenedInfo(it.contactId, it.msgIdHex, it.openedMs.toLong(), it.shredded, it.burnAtMs?.toLong()) },
    fileProgress = fileProgress.map {
        FileProgressInfo(it.transferIdHex, it.contactId, it.incoming, it.name, it.size.toLong(), it.doneChunks.toInt(), it.totalChunks.toInt())
    },
    files = files.map { ReceivedFileInfo(it.transferIdHex, it.contactId, it.name, it.mime, it.bytes) },
    fileFailures = fileFailures.map { FileFailureInfo(it.transferIdHex, it.contactId, it.reason) },
    morePending = morePending,
    restoreNotices = restoreNotices.map { RestoreNoticeInfo(it.contactId, it.restoredAtMs?.toLong(), it.msgIdHex) },
    mediaRequests = mediaRequests.map { it.toApp() },
    resendAsks = resendAsks.map { it.toApp() },
    mediaUpdates = mediaUpdates.map { MediaUpdateInfo(it.contactId, it.msgIdHex, it.state.label()) },
    relayLinks = relayLinks.map { it.toLong() },
)

private class RustLink(private val link: MeshLink) : CoreLink {
    override fun start() = link.start().toResult()
    override fun onPdu(pdu: ByteArray) = link.onPdu(pdu).toResult()
    override fun sendText(text: String) = link.sendText(text).toResult()
    override fun flush() = link.flush().toResult()
    override fun isEstablished() = link.isEstablished()
    override fun handle() = link.handle().toLong()
    override fun close() {
        link.shutdown()
        link.destroy()
    }
}
