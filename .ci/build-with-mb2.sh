#!/bin/bash

set -e

echo "Building for $SFOS_VERSION"

echo "Adding \"*\" as safe directory in git..."
git config --global --add safe.directory "*"

echo "Determine Whisperfish version..."
if [ -z "$CI_COMMIT_TAG" ]; then
    CARGO_VERSION="$(grep -m1 -e '^version\s=\s"' Cargo.toml | sed -e 's/.*"\(.*-dev\).*"/\1/')"
    GIT_REF="$(git rev-parse --short HEAD)"
    VERSION="$CARGO_VERSION.b$CI_PIPELINE_IID.$GIT_REF"
else
    # Strip leading v in v0.6.0- ...
    VERSION=$(echo "$CI_COMMIT_TAG" | sed -e 's/^v//g')
fi
echo "Whisperfish version: $VERSION"

# The MB2 image comes with a default user.
# We need to copy the source over, because of that.

echo "Cloning Whisperfish..."
git clone . ~/whisperfish-build
pushd ~/whisperfish-build

# Determine GIT_VERSION in advance so SFOS targets don't need git
export GIT_VERSION=$(git describe  --exclude release,tag --dirty=-dirty)

# This comes from job cache or the fetch scripy
echo "Restoring ringrtc cache..."
pwd
sudo chown -R "$USER":"$USER" "$CI_PROJECT_DIR/ringrtc"
mv "$CI_PROJECT_DIR/ringrtc" ringrtc

if [ -z "$CARGO_HOME" ]; then
    echo "Warning: CARGO_HOME is not set, default to 'cargo'"
    export CARGO_HOME=cargo
fi

if [ -e "$CI_PROJECT_DIR/cargo" ]; then
    echo "Restoring CARGO_HOME..."
    sudo chown -R "$USER":"$USER" "$CI_PROJECT_DIR/cargo"
    sudo mv "$CI_PROJECT_DIR/cargo" $CARGO_HOME
fi

if [ -e "$CI_PROJECT_DIR/target" ]; then
    echo "Restoring target..."
    sudo chown -R "$USER":"$USER" "$CI_PROJECT_DIR/target"
    sudo mv "$CI_PROJECT_DIR/target" target
fi

git status

rm -rf RPMS

# Set this for sccache.  Sccache is testing out compilers, and host-cc fails here.
TMPDIR2="$TMPDIR"
export TMPDIR="$PWD/tmp/"
mkdir "$TMPDIR"

echo "Configure sccache..."
mkdir -p ~/.config/sccache
cat > ~/.config/sccache/config << EOF
[cache.s3]
bucket = "$SCCACHE_BUCKET"
endpoint = "$SCCACHE_ENDPOINT"
region = "auto"
use_ssl = false
key_prefix = "$SCCACHE_S3_KEY_PREFIX"
no_credentials = false
EOF

# Build vendored / --offline
# These files come from the vendored CI job.
mv "$CI_PROJECT_DIR/vendor.tar.xz" "$CI_PROJECT_DIR/vendor.toml" rpm/

echo "Building Whisperfish for SailfishOS-$SFOS_VERSION-$MER_ARCH..."
mb2 -t "SailfishOS-$SFOS_VERSION-$MER_ARCH" --no-snapshot=force build \
    --enable-debug \
    --no-check \
    -- \
    --define "cargo_version $VERSION" \
    --define "git_version $GIT_VERSION" \
    --without git \
    --with vendor \
    --with lto \
    --with sccache \
    --with tools \
    --with calling \
    2> >(busybox grep -vE "Path not found for FD")

rm -rf "$TMPDIR"
export TMPDIR="$TMPDIR2"

[ "$(ls -A RPMS/*.rpm)" ] || exit 1

# Copy everything useful back

echo "Moving target to cache..."
sudo mv target "$CI_PROJECT_DIR/target"

echo "Moving CARGO_HOME to cache..."
sudo mv $CARGO_HOME "$CI_PROJECT_DIR/cargo"

echo "Moving ringrtc to cache..."
sudo mv ringrtc "$CI_PROJECT_DIR/ringrtc"

popd

mkdir -p RPMS
echo "Moving RPM packages..."
sudo mv -v ~/whisperfish-build/RPMS/* RPMS/

echo "Uploading RPM packages..."
.ci/upload-rpms.sh
echo "Done!"
