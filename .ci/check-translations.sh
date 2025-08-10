#!/bin/sh

TRANSLATION_WARNING="
⚠️ This commit will trigger a change in the 🌐 translation 🌐 files. ⚠️

Make sure to [trigger a Weblate commit](https://hosted.weblate.org/commit/whisperfish/whisperfish-application/) and to [merge the outstanding Weblate merge request](https://gitlab.com/whisperfish/whisperfish/-/merge_requests/) before pulling in this merge request.

Updating the translations will happen *outside* of any merge request, in order to avoid conflicts with Weblate.
"

# lupdate doesn't fail on errors/warnings, so filter the output manually.
# Every not known good row is considered an error.

LOG=$(mktemp)
lupdate qml/ -ts translations/*.ts 2>&1 | tee "$LOG"
LINES=$(sed -E '/^Scanning|^Updating|^    Found|^Removed plural forms|^If this sounds wrong/d' "$LOG" | wc -l)
rm "$LOG"

if [ "$LINES" -gt 0 ]; then
    echo "qmllint reported errors or warnings"
    exit 1
else
    echo "qmllint did not report errors or warnings"
fi

if git diff --exit-code; then
    echo "No translation update needed";
else
    curl --request POST \
        --header "PRIVATE-TOKEN: $PRIVATE_TOKEN" \
        --form "note=$TRANSLATION_WARNING" \
        "$CI_API_V4_URL/projects/$CI_PROJECT_ID/repository/commits/$CI_COMMIT_SHA/comments"
fi
