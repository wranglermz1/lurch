#!/bin/bash
# Setup script for a lurch cloud environment (docs/setup.md). Paste it into the
# environment's Setup script field: it runs as root before Claude Code starts, and its
# result is cached with the environment. Run by hand, it fills in what a session lacks.
# Every step may fail without failing the session.

GHIDRA_VERSION=12.0.4
GHIDRA_ZIP=https://github.com/NationalSecurityAgency/ghidra/releases/download/Ghidra_${GHIDRA_VERSION}_build/ghidra_${GHIDRA_VERSION}_PUBLIC_20260303.zip

if ! command -v zstd >/dev/null 2>&1; then
    (apt-get update -qq && apt-get install -y -qq zstd) >/dev/null 2>&1 &
fi

if [ ! -x /opt/ghidra/support/analyzeHeadless ]; then
    (
        tmp=$(mktemp -d)
        curl -fsSL -o "$tmp/ghidra.zip" "$GHIDRA_ZIP" \
            && unzip -q "$tmp/ghidra.zip" -d "$tmp" \
            && rm -rf /opt/ghidra \
            && mv "$tmp/ghidra_${GHIDRA_VERSION}_PUBLIC" /opt/ghidra
        rm -rf "$tmp"
    ) >/dev/null 2>&1 &
fi

if command -v claude >/dev/null 2>&1; then
    claude plugin marketplace add 'JuliusBrussee/caveman#v3.2.0'
    claude plugin install caveman@caveman
    claude plugin marketplace add 'DietrichGebert/ponytail#v5.1.0'
    claude plugin install ponytail@ponytail
fi >/dev/null 2>&1

wait
exit 0
