#!/bin/sh
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026 Anuna Research
#
# elephant installer.
#
# End users run:
#
#   curl https://files.anuna.io/elephant/install.sh | sh
#
# Detects the platform, downloads the matching prebuilt binary from
# https://files.anuna.io/elephant/, verifies its SHA-256 checksum, and
# installs it to $ELEPHANT_INSTALL_DIR (default: ~/.local/bin). No Rust
# toolchain and no sibling checkouts required. curl downloads do not carry
# macOS's com.apple.quarantine attribute, so notarisation is optional.
#
# Supported artifacts:
#   elephant-darwin-arm64  elephant-darwin-x64  elephant-linux-x64  elephant-linux-arm64
#
# Publishing (operator):
#   Tagging a release with ./release.sh triggers .woodpecker/release.yaml,
#   which cross-compiles the four binaries and uploads them (plus this
#   script as install.sh) to https://files.anuna.io/elephant/. To stage a
#   single-platform artifact by hand, run `make dist` (see the Makefile).
#
# Environment overrides:
#   ELEPHANT_BASE_URL     - artifact base URL (default: https://files.anuna.io/elephant)
#   ELEPHANT_INSTALL_DIR  - install directory (default: ~/.local/bin)
#
# Testing: `INSTALL_SH_TEST=1 . scripts/install.sh` sources the functions
# without running main. See scripts/install_test.sh.

set -u

ELEPHANT_BASE_URL=${ELEPHANT_BASE_URL:-https://files.anuna.io/elephant}

# Print "<os>-<arch>" for the current machine, or fail with a clear
# error on unsupported platforms.
detect_platform() {
    os=$(uname -s)
    arch=$(uname -m)

    case "$os" in
        Darwin) os=darwin ;;
        Linux)  os=linux ;;
        *)
            printf 'error: unsupported operating system: %s\n' "$os" >&2
            printf 'elephant provides prebuilt binaries for macOS and Linux only.\n' >&2
            printf 'Build from source instead: https://git.anuna.io/anuna-research/elephant\n' >&2
            return 1
            ;;
    esac

    case "$arch" in
        arm64 | aarch64) arch=arm64 ;;
        x86_64 | amd64)  arch=x64 ;;
        *)
            printf 'error: unsupported architecture: %s\n' "$arch" >&2
            printf 'elephant provides prebuilt binaries for arm64 and x64 only.\n' >&2
            printf 'Build from source instead: https://git.anuna.io/anuna-research/elephant\n' >&2
            return 1
            ;;
    esac

    printf '%s-%s\n' "$os" "$arch"
}

# Map a "<os>-<arch>" platform string to its artifact name.
resolve_artifact() {
    printf 'elephant-%s\n' "$1"
}

# Verify $1 (file) against $2 (expected SHA-256 hex digest). Skips with
# a warning when no checksum tool is available.
verify_checksum() {
    file=$1
    expected=$2

    if command -v sha256sum >/dev/null 2>&1; then
        actual=$(sha256sum "$file" | awk '{print $1}')
    elif command -v shasum >/dev/null 2>&1; then
        actual=$(shasum -a 256 "$file" | awk '{print $1}')
    else
        printf 'warning: no sha256sum or shasum found; skipping checksum verification\n' >&2
        return 0
    fi

    if [ "$actual" != "$expected" ]; then
        printf 'error: checksum mismatch for %s\n' "$file" >&2
        printf '  expected: %s\n' "$expected" >&2
        printf '  actual:   %s\n' "$actual" >&2
        return 1
    fi
}

main() {
    if ! command -v curl >/dev/null 2>&1; then
        printf 'error: curl is required\n' >&2
        exit 1
    fi

    platform=$(detect_platform) || exit 1
    artifact=$(resolve_artifact "$platform")
    url="$ELEPHANT_BASE_URL/$artifact"
    install_dir=${ELEPHANT_INSTALL_DIR:-$HOME/.local/bin}

    tmpdir=$(mktemp -d) || exit 1
    trap 'rm -rf "$tmpdir"' EXIT INT TERM

    printf 'Downloading %s ...\n' "$url"
    if ! curl -fsSL -o "$tmpdir/elephant" "$url"; then
        printf 'error: download failed: %s\n' "$url" >&2
        exit 1
    fi

    if expected=$(curl -fsSL "$url.sha256" 2>/dev/null); then
        expected=$(printf '%s\n' "$expected" | awk 'NF {print $1; exit}')
        verify_checksum "$tmpdir/elephant" "$expected" || exit 1
    else
        printf 'warning: could not fetch %s.sha256; skipping checksum verification\n' "$url" >&2
    fi

    mkdir -p "$install_dir" || exit 1
    chmod +x "$tmpdir/elephant"
    mv "$tmpdir/elephant" "$install_dir/elephant" || exit 1

    printf 'Installed elephant to %s/elephant\n' "$install_dir"
    case ":$PATH:" in
        *":$install_dir:"*) ;;
        *)
            printf '\n%s is not on your PATH. Add it with:\n' "$install_dir"
            printf '  export PATH="%s:$PATH"\n' "$install_dir"
            ;;
    esac
}

# When sourced with INSTALL_SH_TEST=1 (see scripts/install_test.sh),
# expose the functions without running main.
if [ "${INSTALL_SH_TEST:-}" != "1" ]; then
    main "$@"
fi
