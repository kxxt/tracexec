#!/bin/bash

set -e

# Dependencies are fetched from debian snapshot,
# like: https://snapshot.debian.org/binary/libelf1/


libelf_ver=0.190-1
zlib_ver=1.3.dfsg+really1.3.1-1
wget="wget -c"

rust_arch2deb_arch() {
    case "$1" in
        x86_64)
            echo "amd64"
            ;;
        aarch64)
            echo "arm64"
            ;;
        *)
            echo "$1"
            ;;
    esac
}

unpack_deb_for() {
    mkdir -p "$1"
    dpkg --fsys-tarfile "$2" | tar -C "$1" -x
}

libelf_deb_name_for() {
    if [[ "$2" = "dynamic" ]]; then
        echo "libelf1_${libelf_ver}_$(rust_arch2deb_arch "$1").deb"
    else
        echo "libelf-dev_${libelf_ver}_$(rust_arch2deb_arch "$1").deb"
    fi
}

zlib_deb_name_for() {
    if [[ "$2" = "dynamic" ]]; then
        echo "zlib1g_${zlib_ver}_$(rust_arch2deb_arch "$1").deb"
    else
        echo "zlib1g-dev_${zlib_ver}_$(rust_arch2deb_arch "$1").deb"
    fi
}

prepare_libelf_for() {
    dynamic_deb="$(libelf_deb_name_for "$1" dynamic)"
    $wget "https://snapshot.debian.org/archive/debian/20231117T085632Z/pool/main/e/elfutils/${dynamic_deb}"
    unpack_deb_for "$1" "$dynamic_deb"
    static_deb="$(libelf_deb_name_for "$1" static)"
    $wget "https://snapshot.debian.org/archive/debian/20231117T085632Z/pool/main/e/elfutils/${static_deb}"
    unpack_deb_for "$1" "$static_deb"
}

prepare_zlib_for() {
    timestamp="20240510T150153Z"
    if [[ "$1" = "aarch64" ]]; then
        timestamp="20240510T205852Z"
    elif [[ "$1" = "riscv64" ]]; then
        timestamp="20240510T150153Z"
    fi
    dynamic_deb="$(zlib_deb_name_for "$1" dynamic)"
    $wget "https://snapshot.debian.org/archive/debian/$timestamp/pool/main/z/zlib/${dynamic_deb}"
    unpack_deb_for "$1" "$dynamic_deb"
    static_deb="$(zlib_deb_name_for "$1" static)"
    $wget "https://snapshot.debian.org/archive/debian/$timestamp/pool/main/z/zlib/${static_deb}"
    unpack_deb_for "$1" "$static_deb"
}

ARCH="$1"

if [[ -z "$ARCH" ]]; then
    echo "Usage: $0 <RUST_ARCH>"
    exit 1
fi

prepare_libelf_for "$ARCH"
prepare_zlib_for "$ARCH"
