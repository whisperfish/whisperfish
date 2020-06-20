#!/bin/sh -e

# Source .env for SSH_TARGET
[ -e ".env" ] && source ./.env

# Make sure the `rpm` is up-to-date.
cargo rpm build

# Source the versions from the spec file.
VERSION=$(cat target/armv7-unknown-linux-gnueabihf/release/rpmbuild/SPECS/harbour-whisperfish.spec|egrep '^Version:' | awk '{ print $2 }')
RELEASE=$(cat target/armv7-unknown-linux-gnueabihf/release/rpmbuild/SPECS/harbour-whisperfish.spec|egrep '^Release:' | awk '{ print $2 }')

# Build the RPM filename.
RPM=harbour-whisperfish-$VERSION-$RELEASE.armv7hl.rpm

echo "Copying RPM file ($RPM)"
rsync target/armv7-unknown-linux-gnueabihf/release/rpmbuild/RPMS/armv7hl/$RPM nemo@$SSH_TARGET:/tmp/$RPM

echo "Installing RPM file ($RPM)"
ssh nemo@$SSH_TARGET sdk-deploy-rpm "/tmp/$RPM"

echo Starting harbour-whisperfish
# Use -t to force-allocate a terminal, it triggers Qt to log warnings.
ssh -t nemo@$SSH_TARGET "RUST_BACKTRACE=1 RUST_LOG=harbour_whisperfish=trace,actix=*,awc=*,actix-web=*,libsignal_service=trace,libsignal_service_actix=trace,debug harbour-whisperfish"
