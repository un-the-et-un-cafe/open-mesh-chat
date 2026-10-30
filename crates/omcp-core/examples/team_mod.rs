//! Offline tool for the team key holder (docs/MODERATION.md).
//!
//! ```text
//! team_mod keygen <secret-file>              # writes a new secret key file (0600), prints the PUBLIC key
//! team_mod pub <secret-file>                 # prints the public key
//! team_mod takedown <secret-file> <community_id_hex> <reason 1|2|3|4|5|9> <epoch> [note]
//! team_mod restore  <secret-file> <community_id_hex> <epoch>
//! team_mod takedown-live <secret-file> <stream_id_hex (32 chars)> <reason> <epoch> [note]
//! team_mod restore-live  <secret-file> <stream_id_hex> <epoch>
//! ```
//! Live actions use the same team key and log; the stream id is shown to
//! reporters in the app (Live screen, "Report").
//! `takedown` / `restore` print the signed action as hex, to paste into the
//! app's "Publish team action" field (debug builds) or hand to a gateway.
//! Keep the secret file offline, never in the repository.

use ed25519_dalek::SigningKey;
use omcp_core::moderation::{hex32, Reason, TeamAction};

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
        "takedown-live" | "restore-live" => {
            let k = load(arg(1));
            let sid: [u8; 16] = hex::decode(arg(2))
                .ok()
                .and_then(|b| b.try_into().ok())
                .expect("stream id: 32 hex chars");
            let mut act = if arg(0) == "takedown-live" {
                let r = Reason::from_code(arg(3).parse().expect("reason")).expect("reason code");
                let epoch = arg(4).parse().expect("epoch");
                TeamAction::takedown_live(sid, r, a.get(5).map(String::as_str).unwrap_or(""), epoch)
            } else {
                TeamAction::restore_live(sid, arg(3).parse().expect("epoch"))
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
