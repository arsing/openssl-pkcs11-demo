#!/bin/bash

set -euxo pipefail

# Install deps

case "$CONTAINER_OS" in
    'centos:7')
        case "$OPENSSL_VERSION" in
            '1.0')
                OPENSSL_PACKAGE_NAME='openssl-devel'
                ;;
            *)
                exit 1
                ;;
        esac

        yum install -y curl gcc pkgconfig "$OPENSSL_PACKAGE_NAME"
        ;;

    'debian:9-slim')
        case "$OPENSSL_VERSION" in
            '1.0')
                OPENSSL_PACKAGE_NAME='libssl1.0-dev'
                ;;
            '1.1.0')
                OPENSSL_PACKAGE_NAME='libssl-dev'
                ;;
            *)
                exit 1
                ;;
        esac

        apt-get update
        apt-get install -y curl gcc pkg-config "$OPENSSL_PACKAGE_NAME"
        ;;

    'debian:10-slim')
        case "$OPENSSL_VERSION" in
            '1.1.1')
                OPENSSL_PACKAGE_NAME='libssl-dev'
                ;;
            *)
                exit 1
                ;;
        esac

        apt-get update
        apt-get install -y curl gcc pkg-config "$OPENSSL_PACKAGE_NAME"
        ;;

    *)
        exit 1
esac


# Install Rust

mkdir -p ~/.cargo/bin
curl -Lo ~/.cargo/bin/rustup 'https://static.rust-lang.org/rustup/dist/x86_64-unknown-linux-gnu/rustup-init'
chmod +x ~/.cargo/bin/rustup
export PATH="$PATH:$(realpath ~/.cargo/bin)"

rustup self update
rustup set profile minimal

rustup install stable
rustup default stable

rustup component add clippy

export CARGO_INCREMENTAL=0


cd /src/openssl-pkcs11-demo


# Build

cargo build


# Test

cargo test --all


# Clippy

cargo clippy --all
