use super::*;
use crate::burn::{BurnHeader, BurnStore, Opened, UNREAD_TTL_7D};

const KEY: [u8; 32] = [0x11; 32];
const PEER: [u8; 32] = [0x22; 32];

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty() && hay.windows(needle.len()).any(|w| w == needle)
}

/// Every byte of the store on disk (DB file plus any journal left behind).
fn disk(dir: &Path) -> Vec<u8> {
    let mut all = Vec::new();
    for e in std::fs::read_dir(dir).unwrap() {
        all.extend(std::fs::read(e.unwrap().path()).unwrap());
    }
    all
}

fn msg(id: u8, text: &str, t: u64) -> MessageRec {
    MessageRec {
        id: [id; 16],
        peer: PEER,
        mine: id.is_multiple_of(2),
        time_ms: t,
        status: STATUS_RECEIVED,
        text: text.into(),
        mime: None,
        burn: None,
        burn_state: BURN_NONE,
        burn_at_ms: None,
        file: None,
    }
}

#[test]
fn survives_reopen_and_is_encrypted_at_rest() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("omcp.db");
    {
        let s = Store::open(&path, &KEY).unwrap();
        s.bind_owner(&[1; 32]).unwrap();
        s.put_contact(&ContactRec {
            peer: PEER,
            name: "Zanzibar Quokka".into(),
            verified: true,
            device: None,
            dh: None,
        })
        .unwrap();
        s.put_message(&msg(1, "meet at the old lighthouse", 10))
            .unwrap();
        let mut m2 = msg(2, "", 20);
        m2.file = Some(FileRec {
            name: "holiday-plan.pdf".into(),
            mime: "application/pdf".into(),
            size: 1234,
            state: FILE_RECEIVED,
            done_bytes: 1234,
            local_path: Some("/data/files/holiday-plan.pdf".into()),
            hash: None,
            locator: None,
        });
        s.put_message(&m2).unwrap();
        s.put_outbox(
            &OutboxRec {
                id: [9; 16],
                peer: PEER,
                created_ms: 30,
                inner: b"queued secret".to_vec(),
                control: false,
                burn: false,
            },
            None,
        )
        .unwrap();
        s.put_out_file(
            &OutFileRec {
                id: [5; 16],
                peer: PEER,
                name: "big.bin".into(),
                size: 3,
                total: 2,
                offer: b"offer".to_vec(),
            },
            &[b"chunk-zero".to_vec(), b"chunk-one".to_vec()],
        )
        .unwrap();
    }
    let raw = disk(dir.path());
    for needle in [
        &b"Zanzibar Quokka"[..],
        b"meet at the old lighthouse",
        b"holiday-plan",
        b"queued secret",
        b"chunk-zero",
        &PEER,
        &[1u8; 16],
    ] {
        assert!(!contains(&raw, needle), "plaintext leaked: {needle:?}");
    }

    let s = Store::open(&path, &KEY).unwrap();
    assert_eq!(s.schema_version().unwrap(), SCHEMA_VERSION);
    s.bind_owner(&[1; 32]).unwrap();
    assert!(s.bind_owner(&[2; 32]).is_err());
    let c = s.contacts().unwrap();
    assert_eq!(c.len(), 1);
    assert!(c[0].verified);
    let ms = s.messages(&PEER).unwrap();
    assert_eq!(ms.len(), 2);
    assert_eq!(ms[0].text, "meet at the old lighthouse");
    assert_eq!(
        ms[1].file.as_ref().unwrap().local_path.as_deref(),
        Some("/data/files/holiday-plan.pdf")
    );
    assert_eq!(s.chats().unwrap()[0].last_ms, 20);
    assert_eq!(s.outbox().unwrap()[0].inner, b"queued secret");
    let files = s.out_files().unwrap();
    assert_eq!(files[0].1.len(), 2);
    assert_eq!(&files[0].1[1][..], b"chunk-one");
    s.delete_out_file(&[5; 16]).unwrap();
    assert!(s.out_files().unwrap().is_empty());
}

#[test]
fn update_keeps_order_and_wrong_key_fails_cleanly() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("omcp.db");
    {
        let s = Store::open(&path, &KEY).unwrap();
        s.put_message(&msg(1, "a", 1)).unwrap();
        s.put_message(&msg(2, "b", 2)).unwrap();
        let mut m = msg(1, "a", 1);
        m.status = STATUS_SENT;
        s.put_message(&m).unwrap();
        let ms = s.messages(&PEER).unwrap();
        assert_eq!((ms[0].id, ms[0].status), ([1; 16], STATUS_SENT));
    }
    assert_eq!(
        Store::open(&path, &[0x12; 32]).err(),
        Some(OmcpError::StoreAuth)
    );
}

#[test]
fn rows_are_bound_to_their_id() {
    let s = Store::open_in_memory(&KEY).unwrap();
    s.put_message(&msg(1, "one", 1)).unwrap();
    s.put_message(&msg(2, "two", 2)).unwrap();
    // Swap the ciphertexts of the two rows: both must fail to open.
    s.conn
        .execute_batch(
            "UPDATE messages SET ct = (SELECT ct FROM messages m2 WHERE m2.id <> messages.id)",
        )
        .unwrap();
    assert_eq!(s.messages(&PEER).err(), Some(OmcpError::StoreAuth));
}

#[test]
fn migrations_are_versioned_and_newer_schema_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("omcp.db");
    {
        let s = Store::open(&path, &KEY).unwrap();
        assert_eq!(s.schema_version().unwrap(), 3);
    }
    // Re-opening is idempotent.
    assert_eq!(
        Store::open(&path, &KEY).unwrap().schema_version().unwrap(),
        3
    );
    {
        let c = Connection::open(&path).unwrap();
        c.execute_batch(&format!("PRAGMA user_version = {}", SCHEMA_VERSION + 1))
            .unwrap();
    }
    assert_eq!(
        Store::open(&path, &KEY).err(),
        Some(OmcpError::Store("store schema is newer than this app"))
    );
}

/// REQUIREMENTS B1 / PROTOCOL §19.1: a burned message, its key and its unsent
/// copy are gone from disk, and nothing about it comes back after a restart.
#[test]
fn b1_burned_message_is_not_recoverable_after_restart() {
    const ID: [u8; 16] = [0xB1; 16];
    const ID2: [u8; 16] = [0xB2; 16];
    const SECRET: &[u8] = b"self-destructing rendezvous coordinates";
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("omcp.db");
    let h = BurnHeader::timer(10, UNREAD_TTL_7D).unwrap();

    let (key, sealed) = {
        let s = Store::open(&path, &KEY).unwrap();
        let mut bs = BurnStore::default();
        bs.insert(ID, h, SECRET, 0).unwrap();
        bs.insert(ID2, h, b"another one", 0).unwrap();
        let snap = bs.snapshot(&ID).unwrap();
        s.put_burn(&snap, &PEER, BurnDir::In, None).unwrap();
        s.put_burn(&bs.snapshot(&ID2).unwrap(), &PEER, BurnDir::Out, None)
            .unwrap();
        s.put_outbox(
            &OutboxRec {
                id: ID2,
                peer: PEER,
                created_ms: 0,
                inner: b"unsent burn payload".to_vec(),
                control: false,
                burn: false,
            },
            Some(&bs.snapshot(&ID2).unwrap().key),
        )
        .unwrap();
        let mut m = msg(0xB1, "", 0);
        m.burn = Some(h.encode().to_vec());
        m.burn_state = BURN_UNOPENED;
        m.text = "must never be stored".into();
        s.put_message(&m).unwrap();
        (snap.key.clone(), snap.sealed.clone())
    };
    assert!(!contains(&disk(dir.path()), SECRET));
    assert!(!contains(&disk(dir.path()), b"must never be stored"));

    // Restart #1: the message is still there and opens exactly once.
    {
        let s = Store::open(&path, &KEY).unwrap();
        let mut bs = BurnStore::default();
        for b in s.burns().unwrap() {
            bs.restore(b.snapshot);
        }
        assert_eq!(s.outbox().unwrap()[0].inner, b"unsent burn payload");
        let Opened::Content { body, .. } = bs.open(&ID, 1_000).unwrap() else {
            panic!("expected content");
        };
        assert_eq!(&body[..], SECRET);
        s.put_burn(&bs.snapshot(&ID).unwrap(), &PEER, BurnDir::In, None)
            .unwrap();
        // The read timer runs out (ID) and the unread sent copy expires (ID2):
        // shred them the way the node does.
        assert_eq!(bs.tick(11_000), vec![ID]);
        s.delete_burn(&ID).unwrap();
        for id in bs.tick(u64::from(UNREAD_TTL_7D) * 1000) {
            s.delete_burn(&id).unwrap();
        }
        s.delete_message(&ID).unwrap();
        assert!(bs.is_empty());
    }

    // Restart #2: nothing to restore, and no trace of content, key or ciphertext.
    let s = Store::open(&path, &KEY).unwrap();
    assert!(s.burns().unwrap().is_empty());
    assert!(s.outbox().unwrap().is_empty());
    assert!(s.message(&ID).unwrap().is_none());
    drop(s);
    let raw = disk(dir.path());
    assert!(!contains(&raw, SECRET));
    assert!(!contains(&raw, b"unsent burn payload"));
    assert!(!contains(&raw, &key[..]), "per-message key left on disk");
    assert!(!contains(&raw, &sealed), "burn ciphertext left on disk");
    // Even with the storage key, no burn row remains to decrypt.
    let c = Connection::open(&path).unwrap();
    for t in ["burn", "burn_keys", "outbox"] {
        let n: i64 = c
            .query_row(&format!("SELECT COUNT(*) FROM {t}"), [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0, "{t} not empty");
    }
}

#[test]
fn history_export_excludes_burn_and_imports() {
    let s = Store::open_in_memory(&KEY).unwrap();
    s.put_message(&msg(1, "keep me", 1)).unwrap();
    let mut b = msg(3, "", 2);
    b.burn = Some(
        BurnHeader::on_open(UNREAD_TTL_7D)
            .unwrap()
            .encode()
            .to_vec(),
    );
    s.put_message(&b).unwrap();
    let mut f = msg(5, "", 3);
    f.file = Some(FileRec {
        name: "x".into(),
        mime: "text/plain".into(),
        size: 1,
        state: FILE_RECEIVING,
        done_bytes: 0,
        local_path: Some("/old/device".into()),
        hash: None,
        locator: None,
    });
    s.put_message(&f).unwrap();
    s.put_outbox(
        &OutboxRec {
            id: [7; 16],
            peer: PEER,
            created_ms: 1,
            inner: b"pending".to_vec(),
            control: false,
            burn: false,
        },
        None,
    )
    .unwrap();
    let h = s.export_history().unwrap();

    let t = Store::open_in_memory(&[0x33; 32]).unwrap();
    t.import_history(&h).unwrap();
    let ms = t.messages(&PEER).unwrap();
    assert_eq!(ms.len(), 2);
    assert_eq!(ms[0].text, "keep me");
    let file = ms[1].file.as_ref().unwrap();
    assert_eq!(
        (file.state, file.local_path.as_deref()),
        (FILE_FAILED, None)
    );
    assert_eq!(t.outbox().unwrap().len(), 1);
    assert!(t.import_history(b"junk").is_err());
}

#[test]
fn v014a_rekey_legacy_peer_and_owner() {
    let s = Store::open_in_memory(&KEY).unwrap();
    let legacy_owner = [1; 32];
    let owner = [2; 32];
    s.bind_owner(&legacy_owner).unwrap();
    assert!(s.rebind_owner(&legacy_owner, &owner).unwrap());
    assert!(!s.rebind_owner(&legacy_owner, &owner).unwrap());
    assert!(s.bind_owner(&legacy_owner).is_err());
    s.put_contact(&ContactRec {
        peer: PEER,
        name: "Legacy".into(),
        verified: true,
        device: None,
        dh: None,
    })
    .unwrap();
    s.put_message(&msg(1, "before the upgrade", 10)).unwrap();
    let new = [0x42; 32];
    s.rekey_peer(&PEER, &new).unwrap();
    let cs = s.contacts().unwrap();
    assert_eq!(cs.len(), 1);
    assert_eq!(cs[0].peer, new);
    assert!(cs[0].verified);
    assert!(s.messages(&PEER).unwrap().is_empty());
    let ms = s.messages(&new).unwrap();
    assert_eq!(ms.len(), 1);
    assert_eq!(ms[0].peer, new);
    assert!(s.chat(&new).unwrap().is_some());
    assert!(s.chat(&PEER).unwrap().is_none());
}

#[test]
fn v1_store_migrates_to_kept_media_schema() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("omcp.db");
    {
        let c = Connection::open(&path).unwrap();
        c.execute_batch(MIGRATIONS[0]).unwrap();
        c.execute_batch("PRAGMA user_version = 1").unwrap();
    }
    let s = Store::open(&path, &KEY).unwrap();
    assert_eq!(s.schema_version().unwrap(), 3);
    assert!(s.kept_media_index().unwrap().is_empty());
    assert!(s.carry_items().unwrap().is_empty());
}

#[test]
fn v2_store_migrates_to_relay_table_and_carry_rows_are_sealed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("omcp.db");
    {
        let c = Connection::open(&path).unwrap();
        c.execute_batch(MIGRATIONS[0]).unwrap();
        c.execute_batch(MIGRATIONS[1]).unwrap();
        c.execute_batch("PRAGMA user_version = 2").unwrap();
    }
    let env = b"OPAQUE-ENVELOPE-BYTES-0123456789".to_vec();
    {
        let s = Store::open(&path, &KEY).unwrap();
        assert_eq!(s.schema_version().unwrap(), 3);
        let r = CarryRec {
            id: [7; 16],
            envelope: env.clone(),
            expires_at: 99,
            hop_count: 2,
            own: false,
        };
        s.put_carry(&r).unwrap();
        s.put_mesh_seen(&[8; 16], 50).unwrap();
        assert_eq!(s.carry_items().unwrap(), vec![r]);
        assert_eq!(s.mesh_seen(10).unwrap().len(), 1);
        assert!(s.mesh_seen(60).unwrap().is_empty());
        assert!(s.mesh_seen(0).unwrap().is_empty(), "expired row deleted");
    }
    let raw = std::fs::read(&path).unwrap();
    assert!(!raw.windows(env.len()).any(|w| w == env.as_slice()));
    let s = Store::open(&path, &KEY).unwrap();
    s.delete_carry(&[7; 16]).unwrap();
    assert!(s.carry_items().unwrap().is_empty());
}

#[test]
fn kept_media_sealed_evicts_oldest_and_by_age() {
    let dir = tempfile::tempdir().unwrap();
    let s = Store::open(&dir.path().join("omcp.db"), &KEY).unwrap();
    let day = 86_400_000u64;
    let rec = |n: u8, kept_ms: u64| KeptMediaRec {
        id: [n; 16],
        peer: PEER,
        hash: [n; 32],
        kept_ms,
        size: 1000,
    };
    let lim = KeptMediaLimits {
        max_days: 90,
        max_bytes: 2500,
    };
    let secret = b"KEPT-MEDIA-PLAINTEXT-MARKER".repeat(37);
    let mut r1 = rec(1, day);
    r1.size = secret.len() as u64;
    assert!(s
        .put_kept_media(&r1, &secret, KeptMediaLimits::default(), day)
        .unwrap()
        .is_empty());
    assert!(
        !contains(&disk(dir.path()), b"KEPT-MEDIA-PLAINTEXT-MARKER"),
        "sealed at rest"
    );
    assert_eq!(
        s.kept_media(&[1; 16]).unwrap().unwrap().as_slice(),
        &secret[..]
    );
    s.delete_kept_media(&[1; 16]).unwrap();
    assert!(s.kept_media(&[1; 16]).unwrap().is_none());
    // Size limit: oldest first.
    for n in 1..=3 {
        s.put_kept_media(&rec(n, day * u64::from(n)), &[n; 1000], lim, day * 3)
            .unwrap();
    }
    assert!(s.kept_media(&[1; 16]).unwrap().is_none());
    assert!(s.kept_media(&[2; 16]).unwrap().is_some());
    // Age limit: 30 days later both are kept, after 90 days the older one goes.
    assert!(s.evict_kept_media(lim, day * 32).unwrap().is_empty());
    assert_eq!(
        s.evict_kept_media(lim, day * 92 + 1).unwrap(),
        vec![[2; 16]]
    );
    // Defaults are 90 days / 2 GB.
    assert_eq!(
        KeptMediaLimits::default(),
        KeptMediaLimits {
            max_days: 90,
            max_bytes: 2_000_000_000
        }
    );
    // Re-keyed with the contact.
    s.rekey_peer(&PEER, &[0x33; 32]).unwrap();
    assert_eq!(s.kept_media_index().unwrap()[0].peer, [0x33; 32]);
}
