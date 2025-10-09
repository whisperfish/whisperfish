#!/bin/bash
set -e
DB=whisperfish.sqlite
SQL=whisperfish.sql

if [ -f $DB ]; then
	echo "$DB already exists"
	exit 1;
elif [ -f $SQL ]; then
	echo "$SQL already exists"
	exit 1;
fi

echo "Running migrations..."

for UP in migrations/*/up.sql; do
	echo "$UP"
	sqlite3 $DB < $UP
done

echo "Exporting schema..."
echo ".schema" | sqlite3 $DB > $SQL

