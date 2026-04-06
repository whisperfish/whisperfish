#!/bin/bash

set -e

if [ -z "$CI_COMMIT_TAG" ]; then
    CARGO_VERSION="$(grep -m1 -e '^version\s=\s"' Cargo.toml | sed -e 's/.*"\(.*-dev\).*"/\1/')"
    GIT_REF="$(git rev-parse --short HEAD)"
    if [ -z "$HARBOUR" ]; then
        VERSION="$CARGO_VERSION.b$CI_PIPELINE_IID.$GIT_REF"
    else
        VERSION="$CARGO_VERSION-harbour.b$CI_PIPELINE_IID.$GIT_REF"
    fi
else
    if [ -z "$HARBOUR" ]; then
        # Strip leading v in v0.6.0- ...
        VERSION=$(echo "$CI_COMMIT_TAG" | sed -e 's/^v//g')
    else
        # Strip leading v in v0.6.0- ...
        VERSION=$(echo "$CI_COMMIT_TAG-harbour" | sed -e 's/^v//g')
    fi
fi

# Only upload on tags or main
if [ -n "$CI_COMMIT_TAG" ] || [[ "$CI_COMMIT_BRANCH" == "main" ]]; then
    for RPM_PATH in RPMS/*.rpm; do
        echo Found RPM: $RPM_PATH
        RPM_PATH="${RPM_PATH[0]}"
        RPM=$(basename $RPM_PATH)

        URL="${CI_API_V4_URL}/projects/${CI_PROJECT_ID}/packages/generic/harbour-whisperfish/$VERSION/$RPM"
        echo Posting to $URL

        # Upload to Gitlab
        curl \
             --fail-with-body \
             --header "JOB-TOKEN: $CI_JOB_TOKEN" \
             --upload-file "$RPM_PATH" \
             $URL
    done
fi
