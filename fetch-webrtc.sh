#!/bin/bash

BASE_URL="https://nas.rubdos.be/~rsmet/webrtc/"

# Keep in sync with `whisperfish/build.rs`
declare -A WEBRTC_HASHES
WEBRTC_HASHES[arm]="56d4809b7d034816185b2f165a56514e29a799a6a5be1528a53c72a42990e275bf6c2895076fce991704f9899acfe280"
WEBRTC_HASHES[arm64]="28e0605917aa99b34303ee8b59eb0495b2bb3056ca9be2a5a553d34ac21d067324afd0bef06ac91cb266a7ad04dac4ba"
# WEBRTC_HASHES[x64]=""
WEBRTC_HASHES[x86]="89143eb3464547263770cffc66bb741e4407366ac4a21e695510fb3474ddef4b5bf30eb5b1abac3060b1d9b562c6cbab"

declare -A ARCHS
ARCHS[arm]="armv7-unknown-linux-gnueabihf"
ARCHS[arm64]="aarch64-unknown-linux-gnu"
# ARCHS[]="x86_64"
ARCHS[x86]="i686-unknown-linux-gnu"

for arch in "${!ARCHS[@]}"; do
    hash=${WEBRTC_HASHES[${arch}]}
    echo "Fetching WebRTC for ${arch}..."
    target="ringrtc/${ARCHS[${arch}]}/release/obj/"
    mkdir -p "${target}"

    curl -L "${BASE_URL}/libwebrtc-${arch}-${hash}.a" -o "${target}/libwebrtc.a"
    echo "Verifying WebRTC for ${arch}..."
    echo "${WEBRTC_HASHES[${arch}]}  ${target}/libwebrtc.a" | sha384sum -c
    echo "Done fetching WebRTC for ${arch}."
done
