#!/bin/bash

set -e

echo "Building for $SFOS_VERSION"

echo "Adding \"*\" as safe directory in git..."
git config --global --add safe.directory "*"

# The MB2 image comes with a default user.
# We need to copy the source over, because of that.

echo "Cloning Whisperfish..."
git clone . ~/whisperfish-build
pushd ~/whisperfish-build

git status

cd "shareplugin_v$SHAREPLUGIN_VERSION"

# -f to ignore non-existent files
rm -f RPMS/*.rpm

echo "Building the shareplugin..."
mb2 -t SailfishOS-$SFOS_VERSION-$MER_ARCH --no-snapshot=force build \
    --enable-debug \
    --no-check

[ "$(ls -A RPMS/*.rpm)" ] || exit 1

# Copy everything useful back
popd
echo "Copying RPM files..."
mkdir -p RPMS target
sudo cp -ar ~/whisperfish-build/shareplugin_v$SHAREPLUGIN_VERSION/RPMS/* RPMS/

echo "Uploading RPM files to GitLab..."
.ci/upload-rpms.sh
echo "Done!"
