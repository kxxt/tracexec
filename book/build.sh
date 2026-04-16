#!/bin/bash

# Shell script for vercel build.

set -ex

curl -Lo mdbook.tar.gz https://github.com/rust-lang/mdBook/releases/download/v0.5.4/mdbook-v0.5.4-x86_64-unknown-linux-musl.tar.gz
tar -xvzf mdbook.tar.gz
curl -Lo mdbook-asciinema.tar.gz https://github.com/github/mdbook-asciinema/releases/download/v0.5.0/mdbook-asciinema-v0.5.0-x86_64-unknown-linux-musl.tar.gz
tar -xvzf mdbook-asciinema.tar.gz

PATH="$PWD:$PATH" mdbook build