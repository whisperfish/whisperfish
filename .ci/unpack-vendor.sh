#!/bin/bash

echo 'Unpacking vendor and .cargo/config.toml...'
mkdir -p .cargo
cp -v vendor.toml .cargo/config.toml
tar xf vendor.tar.xz
echo "vendor/ has $(find vendor/ -type f | wc -l) files"
