#!/bin/sh

# This script launches Whisperfish, and is used to decide
# whether Whisperfish should be launched in SailJail'd mode or not.

if [ -e "/usr/bin/sailjail" ]; then
    /usr/bin/sailjail \
        -p harbour-whisperfish.desktop \
        /usr/bin/harbour-whisperfish "$@"
else
    invoker \
        --type=qtquick2 \
        --single-instance \
        /usr/bin/harbour-whisperfish "$@"
fi
