#!/bin/bash
LUPDATE=$(which lupdate 2>/dev/null || which lupdate-qt5 2>/dev/null)
if [ ! -f "$LUPDATE" ]; then
  echo "lupdate or lupdate-qt5 not found in \$PATH"
  exit 1
fi

$LUPDATE qml/ -noobsolete -ts translations/*.ts
