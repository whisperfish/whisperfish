#!/bin/sh -exu

version="$1"
archs="aarch64 i486 armv7hl"

# Download
for arch in $archs; do
    [ -e "$version-$arch.zip" ] && continue
    curl -LvJO "https://gitlab.com/whisperfish/whisperfish/-/jobs/artifacts/$version/download?job=build:$arch"
    mv artifacts.zip $version-$arch.zip
done

# Unzip into releases dir
mkdir -p releases
for arch in $archs; do
    unzip -od "$version-$arch" $version-$arch.zip
    for F in $(fd -I harbour-whisperfish $version-$arch); do
        mv "$F" releases/
    done
done

# Cleanup
for arch in $archs; do
    rm -rf "$version-$arch" "$version-$arch".zip
done
