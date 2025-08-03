#!/bin/bash

BASE_URL="https://nas.rubdos.be/~rsmet/webrtc/"

# Keep in sync with `whisperfish/build.rs`
declare -A WEBRTC_HASHES

# Hashes for webrtc built with OpenSSL 3.2.2
WEBRTC_HASHES[322_arm]="56e28c6c02fec08dd6b39eab5d08b43fcb50342b0328cb127962b794ecb2c0b0031e0846c2318fe1efcac65363c74e1a"
WEBRTC_HASHES[322_arm64]="fc325ad89677706d61c7fed82f2ff753f591f93636f6ab615a5042fdd4ba681cc1aed70e0d5ce1d22391957640efd11f"
WEBRTC_HASHES[322_x64]="29db5abda6f5a9ccfa4d748f295a16b212b275bcf1441ac3856de6ee6cff855b89e6cf3a510d4da4d0abdcbcd3553434"
WEBRTC_HASHES[322_x86]="3752471a15b21dc40703e9a00bc7de2a18e3a60bb8a76c8c18665aa4a4cf14b7e7674e4d0342a051516bbbf63e16adfc"

# Hashes for webrtc built with OpenSSL 1.1.1
WEBRTC_HASHES[111_arm]="56d4809b7d034816185b2f165a56514e29a799a6a5be1528a53c72a42990e275bf6c2895076fce991704f9899acfe280"
WEBRTC_HASHES[111_arm64]="28e0605917aa99b34303ee8b59eb0495b2bb3056ca9be2a5a553d34ac21d067324afd0bef06ac91cb266a7ad04dac4ba"
WEBRTC_HASHES[111_x64]="337860360916a03c0a0da3e44f002f9cf3083c38ad4b4de9a9052a6ff50c9fc909433cabccaf6075554056d29408558f"
WEBRTC_HASHES[111_x86]="89143eb3464547263770cffc66bb741e4407366ac4a21e695510fb3474ddef4b5bf30eb5b1abac3060b1d9b562c6cbab"

declare -A ARCHS
if [ "$1" == "aarch64" ] || [ -z "$1" ]; then
    ARCHS[arm64]="aarch64-unknown-linux-gnu"
fi
if [ "$1" == "armv7hl" ] || [ -z "$1" ]; then
    ARCHS[arm]="armv7-unknown-linux-gnueabihf"
fi
if [ "$1" == "i486" ] || [ -z "$1" ]; then
    ARCHS[x86]="i686-unknown-linux-gnu"
fi
if [ "$1" == "x86_64" ] || [ -z "$1" ]; then
    ARCHS[x64]="x86_64-unknown-linux-gnu"
fi

for arch in "${!ARCHS[@]}"; do
    for version in 111 322; do
        hash=${WEBRTC_HASHES[${version}_${arch}]}
        target="ringrtc/$version/${ARCHS[${arch}]}/release/obj"
        debug_target="ringrtc/$version/${ARCHS[${arch}]}/debug/obj"
        mkdir -p "${target}" "${debug_target}"

        if [ -f "${target}/libwebrtc.a" ]; then
            echo "${hash} ${target}/libwebrtc.a" | sha384sum -c >/dev/null && continue || echo "Hash mismatch, refetching..."
        fi

        echo "Fetching WebRTC (OpenSSL ${version}) for ${arch}..."

        curl -L "${BASE_URL}/libwebrtc-${arch}-${hash}.a" -o "${target}/libwebrtc.a"
        echo "Verifying WebRTC for ${arch}..."
        echo "$hash  ${target}/libwebrtc.a" | sha384sum -c
        cp "${target}/libwebrtc.a" "${debug_target}/libwebrtc.a"
        echo "Done fetching WebRTC for ${arch}."
    done
done
