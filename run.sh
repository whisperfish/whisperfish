#!/bin/bash
set -eu
cd "$(dirname -- "$(type greadlink >/dev/null 2>&1 && greadlink -f -- "$0" || readlink -f -- "$0")")"

# Source .env for SSH_TARGET
[ -e ".env" ] && source ./.env
: ${TARGET_ARCH?Please update your .env file to include the TARGET_ARCH export}

qmllint qml/**/*.qml

# Make sure the `rpm` is up-to-date.
cargo rpm build --target="${TARGET_ARCH}"

# Query the effective Cargo target directory
TARGET_DIR="$(cargo metadata --format-version=1 | jq -r ".target_directory")"

# Source the versions from the spec file.
VERSION=$(cat "${TARGET_DIR}/${TARGET_ARCH}/release/rpmbuild/SPECS/harbour-whisperfish.spec" | egrep '^Version:' | awk '{ print $2 }')
RELEASE=$(cat "${TARGET_DIR}/${TARGET_ARCH}/release/rpmbuild/SPECS/harbour-whisperfish.spec" | egrep '^Release:' | awk '{ print $2 }')

# Map relevant Rust architecture names to their RPM equivalents.
# Based on https://github.com/iqlusioninc/cargo-rpm/blob/aaa92d04205634a4221b6fc23ea9d3b71d3f2b74/src/target_architecture.rs
case "${TARGET_ARCH}" in
    aarch64-*)         RPM_ARCH=aarch64 ;;
	armv7-*-gnueabihf) RPM_ARCH=armv7hl ;;
	armv7-*-gnueabi)   RPM_ARCH=armv7   ;;
	x86-*)             RPM_ARCH=i486    ;;

	*)
		echo "Unrecognized target architecture ${TARGET_ARCH}! Please edit ./run.sh and submit your changes." >&2
		exit 1
	;;
esac

# Build the RPM filename.
RPM="harbour-whisperfish-${VERSION}-${RELEASE}.${RPM_ARCH}.rpm"

echo "Copying RPM file (${RPM})"
rsync "${TARGET_DIR}/${TARGET_ARCH}/release/rpmbuild/RPMS/${RPM_ARCH}/${RPM}" "nemo@${SSH_TARGET}:/tmp/${RPM}"

echo "Installing RPM file (${RPM})"
ssh "nemo@${SSH_TARGET}" sdk-deploy-rpm "/tmp/${RPM}"

echo "Starting harbour-whisperfish"
# Use -t to force-allocate a terminal, it triggers Qt to log warnings.
ssh -t "nemo@${SSH_TARGET}" "RUST_BACKTRACE=full harbour-whisperfish --verbose"
