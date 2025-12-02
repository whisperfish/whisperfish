#!/bin/bash
# shellcheck disable=SC2155
set -e

if which llvm-profdata >/dev/null ; then
	export LLVM_TOOL=$(which llvm-profdata)
else
	echo "Error: llvm-profdata not found"
	exit 1
fi

if ! which grcov >/dev/null ; then
	echo "Error: grcov not found"
	exit 1
fi

export QMAKE=/usr/bin/qmake
qmake_is_qt5=$($QMAKE --version | grep "Qt version 5." | wc -l)
if [ $qmake_is_qt5 -ne 1 ] ; then
	echo "Error: $QMAKE is not using Qt5"
	exit 1
fi

export LLVM_PATH=$(dirname "${LLVM_TOOL}")
export RUSTFLAGS="-Cinstrument-coverage -Ccodegen-units=1 -Copt-level=0 -Clink-dead-code -Coverflow-checks=off"
export LLVM_PROFILE_FILE="whisperfish-%p-%m.profraw"
export CARGO_TARGET_DIR="coverage"

cargo test

mkdir -p coverage/html

echo "Running grcov, this takes a little while..."

grcov . \
	--binary-path ./coverage/debug/deps/ \
	--branch \
	--ignore-not-existing \
	--ignore '../*' \
	--ignore "/*" \
	--ignore "whisperfish/tests/*" \
	--ignore 'target/*' \
	--ignore 'coverage/*' \
	--llvm-path "${LLVM_PATH}" \
	--excl-start "#\[cfg\(test\)\]" \
	-s . \
	-t html \
	-o coverage/html

find . -name "*.profraw" -delete

if [ -f "coverage/html/index.html" ]; then
	echo "Coverage report available in $(realpath "coverage/html/index.html")"
fi
