//! Offline tool for the team key holder (docs/MODERATION.md).
//!
//! ```text
//! team_mod keygen <secret-file>              # writes a new secret key file (0600), prints the PUBLIC key
//! team_mod pub <secret-file>                 # prints the public key
//! team_mod takedown <secret-file> <community_id_hex> <reason 1-7|9> <epoch> [note]
//! team_mod restore  <secret-file> <community_id_hex> <epoch>
//! team_mod v2 <secret-file> <kind 3-8> <target_hex> <item_hex|0> <reason|0> <evidence_hash_hex|0> <epoch> [note]
//! ```
//! v2 kinds (PROTOCOL §23.10.7): 3 GROUP_TAKEDOWN / 4 GROUP_RESTORE (target =
//! group_ref, item 0), 5 POST_, 6 TOPIC_, 7 LIVE_TAKEDOWN, 8 ITEM_RESTORE
//! (target = community_id, item = post/topic/stream id, left-padded).
//! `takedown` / `restore` print the signed action as hex, to paste into the
//! app's "Publish team action" field (debug builds) or hand to a gateway.
//! Keep the secret file offline, never in the repository.

use ed25519_dalek::SigningKey;
use omcp_core::moderation::{hex32, pad_id, ActionKind, Reason, TeamAction};

fn load(path: &str) -> SigningKey {
    let s = std::fs::read_to_string(path).expect("read secret file");
    SigningKey::from_bytes(&hex32(s.trim()).expect("secret file: 64 hex chars"))
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let arg = |i: usize| a.get(i).map(String::as_str).unwrap_or_else(|| usage());
    match arg(0) {
        "keygen" => {
            let path = arg(1);
            let mut b = [0u8; 32];
            getrandom::getrandom(&mut b).expect("rng");
            let mut opts = std::fs::OpenOptions::new();
            opts.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                opts.mode(0o600);
            }
            use std::io::Write;
            let mut f = opts
                .open(path)
                .expect("create secret file (must not exist)");
            writeln!(f, "{}", hex::encode(b)).expect("write");
            println!(
                "{}",
                hex::encode(SigningKey::from_bytes(&b).verifying_key().to_bytes())
            );
        }
        "pub" => println!("{}", hex::encode(load(arg(1)).verifying_key().to_bytes())),
        "takedown" | "restore" => {
            let k = load(arg(1));
            let c = hex32(arg(2)).expect("community id: 64 hex chars");
            let mut act = if arg(0) == "takedown" {
                let r = Reason::from_code(arg(3).parse().expect("reason")).expect("reason code");
                let epoch = arg(4).parse().expect("epoch");
                TeamAction::takedown(c, r, a.get(5).map(String::as_str).unwrap_or(""), epoch)
            } else {
                TeamAction::restore(c, arg(3).parse().expect("epoch"))
            };
            act.sign(&k);
            println!("{}", hex::encode(act.encode().expect("valid action")));
        }
        "v2" => {
            let k = load(arg(1));
            let kind = ActionKind::from_code(arg(2).parse().expect("kind")).expect("kind 3-8");
            let h = |s: &str| {
                if s == "0" {
                    [0u8; 32]
                } else {
                    pad_id(&hex::decode(s).expect("hex"))
                }
            };
            let rc: u8 = arg(5).parse().expect("reason");
            let mut act = TeamAction {
                kind,
                target_id: h(arg(3)),
                item_id: h(arg(4)),
                reason: Reason::from_code(rc),
                evidence_hash: h(arg(6)),
                note: a.get(8).cloned().unwrap_or_default(),
                epoch: arg(7).parse().expect("epoch"),
                sigs: Vec::new(),
            };
            act.sign(&k);
            println!("{}", hex::encode(act.encode().expect("valid action")));
        }
        _ => usage(),
    }
}

fn usage() -> ! {
    eprintln!("usage: see the doc comment in crates/omcp-core/examples/team_mod.rs");
    std::process::exit(2)
}
