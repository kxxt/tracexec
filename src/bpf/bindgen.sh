#!/bin/sh

bindgen --allowlist-type exec_event_flags \
    -o interface.rs interface.h -- \
    -I ../../include -target bpf -D__x86_64__ # arch really doesn't matter here as long as it's 64bit

sed -i '1apub use imp::*;\n#[allow(dead_code)]\n#[allow(non_snake_case)]\n#[allow(non_camel_case_types)]\n#[allow(non_upper_case_globals)]\nmod imp {' \
    interface.rs

echo '}' >> interface.rs