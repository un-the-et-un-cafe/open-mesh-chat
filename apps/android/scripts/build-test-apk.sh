#!/usr/bin/env bash
# Sideload test APK (~35 MB): package chat.omcp.app.debug, arm64 only.
# Built as a *release* variant (merged, non-debuggable dex: 5 files) with the
# `.debug` id suffix, not `assembleDebug`: debug builds keep per-library dex
# (17 files, ~+4 MB) and the APK grows to ~39 MB. Then other ABIs are dropped,
# .so files stripped, aligned and signed with the local debug keystore.
# Usage: scripts/build-test-apk.sh <out.apk>   (needs ANDROID_HOME, ANDROID_NDK_HOME)
set -euo pipefail
HERE="$(cd "$(dirname "$0")/.." && pwd)"
OUT="$(realpath -m "${1:?out.apk}")"
OMCP_ABIS=arm64-v8a bash "$HERE/scripts/build-rust.sh"
(cd "$HERE" && ./gradlew -q assembleRelease -POMCP_RELEASE_APP_ID_SUFFIX=.debug)
BT="$(ls -d "$ANDROID_HOME"/build-tools/* | tail -1)"
STRIP="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/llvm-strip"
TMP="$(mktemp -d)"
python3 - "$HERE/app/build/outputs/apk/release/app-release-unsigned.apk" "$TMP/u.apk" "$STRIP" <<'PY'
import zipfile, subprocess, sys
src, out, strip = sys.argv[1:4]
zi, zo = zipfile.ZipFile(src), zipfile.ZipFile(out, 'w')
for i in zi.infolist():
    n = i.filename
    if n.startswith('META-INF/') and n.split('.')[-1] in ('SF', 'RSA', 'EC', 'DSA', 'MF'): continue
    if n.startswith('lib/') and not n.startswith('lib/arm64-v8a/'): continue
    d = zi.read(n)
    if n.endswith('.so'):
        t = out + '.so'; open(t, 'wb').write(d); subprocess.check_call([strip, '--strip-unneeded', t]); d = open(t, 'rb').read()
    stored = n.endswith('.so') or n == 'resources.arsc'
    zo.writestr(zipfile.ZipInfo(n, date_time=i.date_time), d, compress_type=zipfile.ZIP_STORED if stored else zipfile.ZIP_DEFLATED)
zo.close()
PY
"$BT/zipalign" -f -p 4 "$TMP/u.apk" "$TMP/a.apk"
"$BT/apksigner" sign --ks "$HOME/.android/debug.keystore" --ks-pass pass:android --out "$OUT" "$TMP/a.apk"
ls -l "$OUT"
