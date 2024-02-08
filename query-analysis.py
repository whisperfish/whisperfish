#!/usr/bin/env python3

# How to use:
# 1. Run Whisperfish with diesel-instrumentation enabled, e.g. `sfdk build --with diesel_instrumentation`
# 2. Run Whisperfish and perform some actions
# 3. Close Whisperfish gracefully.  It will output the queries to stdout.
# 4. Copy the queries to `queries.log`
# 5. Ensure test.db is a test database with the same schema as Whisperfish, `diesel migration run` should take care of that
# 6. Run this script

import sqlite3

queries = open('queries.log', 'r').read().split('\n')

db = sqlite3.connect('test.db')

for query in queries:
    if query.strip() == '':
        continue
    assert query.startswith('Query: '), query
    assert query.endswith('times'), query
    query = query[7:-6]
    [query, times] = query.split(' was executed ')
    times = int(times)

    bindings = query.count('?')

    bindings = [0 for _ in range(bindings)]

    res = db.execute('EXPLAIN QUERY PLAN ' + query, bindings).fetchall()
    has_scan = any('SCAN' in r[3] for r in res)
    if has_scan:
        print(f'Query with SCAN: {query} is executed times: {times}')
        print(times, res)
        print()

    # print(f'Query: {query}, Times: {times}')
