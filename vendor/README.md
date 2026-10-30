# vendor/

## openmls (0.9.2 + OMCP patch)

**Why.** Reports from private groups (PROTOCOL §23.10.1) must let the team
re-check the *sender's own MLS signature* on a reported message. openmls 0.9.2
(the latest release) verifies that signature while decrypting but never exposes
the signed bytes. Without them a reporter could only offer plaintext, which any
member can forge.

**What.** `vendor/openmls` is the crates.io `openmls-0.9.2` source, wired in via
`[patch.crates-io]` in the workspace `Cargo.toml`. Removed from the copy:
`benches/`, `examples/`, `tests/`, `Cargo.lock` and the matching
`[[bench]]`/`[[example]]`/`[[test]]` sections of its `Cargo.toml`. Added
(read-only, no behaviour change):

- `src/framing/validation.rs`: `pub struct SignedContentExport { framed_content,
  wire_format, group_context, signature, sender_leaf_node }`, an optional field
  on `ProcessedMessage`, `ProcessedMessage::signed_content()` and a crate-private
  setter.
- `src/framing/mls_auth_content.rs`: crate-private `framed_content_bytes()`
  (TLS-serialised `FramedContent`).
- `src/ciphersuite/signature.rs`: crate-private `raw_bytes()`.
- `src/group/mls_group/processing.rs`: for application messages from a member
  in the current epoch, fill the export after the signature was verified.

The exact diff is `openmls-0.9.2-omcp.patch` (also covers the removed
`Cargo.toml` sections). To check it against upstream:

```sh
cargo download openmls==0.9.2   # or unpack ~/.cargo/registry/src/*/openmls-0.9.2
diff -ru -x benches -x examples -x tests -x Cargo.lock -x Cargo.toml.orig \
  openmls-0.9.2 vendor/openmls
```

**Remove it** (drop the directory and the `[patch.crates-io]` entry) as soon as
an openmls release exposes the signed content / signature of processed
application messages. The OMCP side only uses `SignedContentExport` in
`crates/omcp-core/src/mls/group.rs`.

License: openmls is MIT (see `license` in its `Cargo.toml`; the crates.io package ships no separate LICENSE file).
