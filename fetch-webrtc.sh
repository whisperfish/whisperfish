#!/bin/bash

BASE_URL="https://nas.rubdos.be/~rsmet/webrtc/"

# Keep in sync with `whisperfish/build.rs`
declare -A WEBRTC_HASHES

# Hashes for webrtc built with OpenSSL 3.6.5
WEBRTC_HASHES[365_arm]="7a86dacfc4dea3fd94284ee22c6221ed9c4858ae23cd05bea8c335f25d0f3881ecf92fceacee346b2201e2205af79c3f"
WEBRTC_HASHES[365_arm64]="0de6c8cec509cbaa7f5a33760ae87fdfac0e8e80c63247572064da11cf24ee5bdccc258f29486d016b2883fd8f2152c1"
WEBRTC_HASHES[365_x64]="9cf4d8b9d036c784f92a1b8216879bb71f4939329eca09cace6f1496dda1af06a141c2c304d6e5d0b857755e542a6945"
WEBRTC_HASHES[365_x86]="0702a115ff693eef0ba4534e2af9de43c3eaebf7bae58bfaf108a35ee5c0199e0c6e5adf6f0d7ef6ef4964be3f07c875"

# Hashes for webrtc built with OpenSSL 3.5.8
WEBRTC_HASHES[358_arm]="7eccc8fc8347dce26d1f54a67d25c4b137035b73f59816a0cc30d79cfb0d9da5e5b39334b9564025c142cedc3b56a35d"
WEBRTC_HASHES[358_arm64]="7d1bf9a285ef55c4547fe013b6660e0600afd7fa3ac895af000264cde6d1b22891d42e970da892e5c809d71f1dbb4660"
WEBRTC_HASHES[358_x64]="dc077a619de00a7c57c5d87dcedcec59656bf51f7d02b733bdf851defc9496e71124b948f22455410a045d180053ffbd"
WEBRTC_HASHES[358_x86]="a04061f6c83d407bc1ace23109ba4772a2d1ba2c3346f34d93c191a5947ccb0119eaba3ba4ab1e6b543e3c161c0fa0b2"

# Hashes for webrtc built with OpenSSL 1.1.1
WEBRTC_HASHES[111_arm]="3ee0e141db54729e3224f95032f02672f277fe462536d734bf4c21c42e4cad373e773a7ee327222bd1aba2b907bf9443"
WEBRTC_HASHES[111_arm64]="a30ec6e40c37fb61ec608373d68b2ce4e8c82cbdacc05967e37a293fe8f9bf16266d88b9b408ca409027d806bcb7d269"
WEBRTC_HASHES[111_x64]="1c1e0cf95316e914bf73a7962f92c00b5fd5d4297553a86b3693f62dfcec12cd34664a7d0b3ebd56188652322ebc8092"
WEBRTC_HASHES[111_x86]="697e6c14f3ee32bd80f69db12e062f6be16fedfbc9969911fc4a85e9004c801a8d59aea0b1c04d94cd183098f404dbe8"

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
    for version in 111 358 365; do
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
