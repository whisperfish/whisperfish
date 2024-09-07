#!/bin/bash

set -e

echo "Building for $SFOS_VERSION"

sudo zypper install -y \
    zlib-devel \

# Tooling-side dependencies used in build.rs
sdk-manage tooling maintain SailfishOS-$SFOS_VERSION \
    zypper install -y \
        zlib-devel \

echo adding $PWD as safe directory in git
git config --global --add safe.directory $PWD

if [ -z "$CI_COMMIT_TAG" ]; then
    CARGO_VERSION="$(grep -m1 -e '^version\s=\s"' whisperfish/Cargo.toml | sed -e 's/.*"\(.*-dev\).*"/\1/')"
    GIT_REF="$(git rev-parse --short HEAD)"
    VERSION="$CARGO_VERSION.b$CI_PIPELINE_IID.$GIT_REF"
else
    # Strip leading v in v0.6.0- ...
    VERSION=$(echo "$CI_COMMIT_TAG" | sed -e 's/^v//g')
fi

# The MB2 image comes with a default user.
# We need to copy the source over, because of that.

git clone . ~/whisperfish-build
pushd ~/whisperfish-build

bash fetch-webrtc.sh

# We also need to move the cache, and afterwards move it back.
if [ -e "$CI_PROJECT_DIR/cargo" ]; then
    sudo mv $CI_PROJECT_DIR/cargo ~/cargo
    sudo chown -R $USER:$USER ~/cargo
fi

git status

# -f to ignore non-existent files
rm -f RPMS/*.rpm

# Set this for sccache.  Sccache is testing out compilers, and host-cc fails here.
TMPDIR2="$TMPDIR"
export TMPDIR=$PWD/tmp/
mkdir $TMPDIR

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

MAJOR_VERSION=$(echo $TARGET_VERSION | awk -F. '{print $1 FS $2}')

mb2 -t SailfishOS-$TARGET_VERSION-$MER_ARCH build \
    --enable-debug \
    --no-check \
    -- \
    --define "cargo_version $VERSION"\
    --with lto \
    --with sccache \
    --with tools \

rm -rf $TMPDIR
export TMPDIR="$TMPDIR2"


[ "$(ls -A RPMS/*.rpm)" ] || exit 1

# Copy everything useful back
popd
mkdir -p RPMS target
sudo cp -ar ~/whisperfish-build/RPMS/* RPMS/
sudo cp -ar ~/whisperfish-build/target/* target/

sudo mv ~/cargo $CI_PROJECT_DIR/cargo

.ci/upload-rpms.sh
