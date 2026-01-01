#!/bin/bash

echo 'Unpacking vendor and .cargo/config.toml...'
tar xf vendor.tar.xz
find vendor/ -type f
mkdir -p .cargo
cp -v vendor.toml .cargo/config.toml
