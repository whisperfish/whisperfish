#!/bin/bash

set -e

echo_t() {
    echo "[$(date +%H:%M:%S)]" "$@"
}

echo_t "Building for $SFOS_VERSION"

echo_t "Adding \"*\" as safe directory in git..."
git config --global --add safe.directory "*"

echo_t "Determine Whisperfish version..."
if [ -z "$CI_COMMIT_TAG" ]; then
    CARGO_VERSION="$(grep -m1 -e '^version\s=\s"' Cargo.toml | sed -e 's/.*"\(.*-dev\).*"/\1/')"
    GIT_REF="$(git rev-parse --short HEAD)"
    VERSION="$CARGO_VERSION.b$CI_PIPELINE_IID.$GIT_REF"
else
    # Strip leading v in v0.6.0- ...
    VERSION=$(echo "$CI_COMMIT_TAG" | sed -e 's/^v//g')
fi
echo_t "Whisperfish version: $VERSION"

# The MB2 image comes with a default user.
# We need to copy the source over, because of that.

echo_t "Cloning Whisperfish..."
git clone . ~/whisperfish-build

# Determine GIT_VERSION in advance so SFOS targets don't need git
export GIT_VERSION=$(git describe  --exclude release,tag --dirty=-dirty)

# This comes from job scripts
echo_t "Restoring ringrtc cache..."
sudo chown -R "$USER":"$USER" "$CI_PROJECT_DIR/ringrtc"
[ -f "./whisperfish-build/ringrtc" ] && rm -rf "./whisperfish-build/ringrtc"
mv -v "$CI_PROJECT_DIR/ringrtc" "./whisperfish-build/ringrtc"

pushd ~/whisperfish-build

git status

# -f to ignore non-existent files
rm -f RPMS/*.rpm

# Set this for sccache.  Sccache is testing out compilers, and host-cc fails here.
TMPDIR2="$TMPDIR"
export TMPDIR="$PWD/tmp/"
mkdir "$TMPDIR"

echo_t "Configure sccache..."
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

echo_t "Building Whisperfish for SailfishOS-$SFOS_VERSION-$MER_ARCH..."
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
popd

mkdir -p RPMS
echo_t "Moving RPM packages..."
sudo mv -v ~/whisperfish-build/RPMS/* RPMS/

echo_t "Moving ringrtc for cache..."
sudo mv ~/whisperfish-build/ringrtc "$CI_PROJECT_DIR/ringrtc"

echo_t "Uploading RPM packages..."
.ci/upload-rpms.sh
echo_t "Done!"
