#!/bin/bash

mkdir -p $CARGO_HOME
cp .ci/cargo.toml $CARGO_HOME/config

echo "Building for $SFOS_VERSION"
echo "Configuring cargo-rpm (cfr https://gitlab.com/rubdos/whisperfish/-/issues/24)"

if [ -z "$CI_COMMIT_TAG" ]; then
    CARGO_VERSION="$(grep -m1 -e '^version\s=\s"' Cargo.toml | sed -e 's/.*"\(.*\)"/\1/')"
    GIT_REF="$(git rev-parse --short HEAD)"
    VERSION="$CARGO_VERSION.b$CI_PIPELINE_IID.$GIT_REF"
else
    # Strip leading v in v0.6.0- ...
    VERSION=$(echo "$CI_COMMIT_TAG" | sed -e 's/^v//g')
fi

# Configure Cargo.toml
sed -ie "s/armv7hl/$MER_ARCH/" Cargo.toml
sed -ie "s/^version\s\?=\s\?\".*\"/version = \"$VERSION\"/" Cargo.toml
cat Cargo.toml

# Set env
export MER_TARGET="SailfishOS-$SFOS_VERSION"
export RUSTFLAGS="-C lto=fat -C link-args=-Wl,-lcrypto,-rpath-link,$MERSDK/targets/$MER_TARGET-$MER_ARCH/usr/lib64/,-rpath-link,$MERSDK/targets/$MER_TARGET-$MER_ARCH/usr/lib/,-rpath-link,$MERSDK/targets/$MER_TARGET-$MER_ARCH/lib/,-rpath-link,$MERSDK/targets/$MER_TARGET-$MER_ARCH/lib64/"

rustc --version
cargo --version

rm target/*/release/rpmbuild/RPMS/*/*.rpm

cargo-rpm --help
cargo rpm build --verbose --target $RUST_ARCH

# Only upload on tags or master
if [ -n "$CI_COMMIT_TAG" ] || [[ "$CI_COMMIT_BRANCH" == "master" ]]; then
    RPM_PATH=(target/*/release/rpmbuild/RPMS/*/*.rpm)
    RPM_PATH="${RPM_PATH[0]}"
    RPM=$(basename $RPM_PATH)

    BASEVERSION=$(echo $VERSION | sed -e 's/\([0-9]\+\.[0-9]\+\.[0-9]\+\).*/\1/')

    URL="$CI_API_V4_URL/projects/$CI_PROJECT_ID/packages/generic/harbour-whisperfish/$BASEVERSION/$RPM"
    echo Posting to $URL

    # Upload to Gitlab
    curl --header "PRIVATE-TOKEN: $PRIVATE_TOKEN" \
         --upload-file "$RPM_PATH" \
         $URL
fi
