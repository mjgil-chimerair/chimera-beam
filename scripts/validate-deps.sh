#!/bin/bash
# Dependency graph validation script for rustzigbeam
# Validates that dependency rules are not violated

set -e

echo "Validating crate dependency graph..."

# Get workspace packages as array
mapfile -t PACKAGES < <(cargo metadata --format-version 1 --no-deps 2>/dev/null | jq -r '.packages[] | select(.name | startswith("rustzigbeam")) | .name')

# Rule 1: Only rustzigbeam_abi can have FFI extern "C" blocks linking beamz
for pkg in "${PACKAGES[@]}"; do
    if [[ "$pkg" != "rustzigbeam_abi" ]]; then
        if grep -r "extern.*C.*beamz" "crates/$pkg/src/" 2>/dev/null; then
            echo "ERROR: $pkg links Zig directly (only rustzigbeam_abi should)"
            exit 1
        fi
    fi
done

# Rule 2: Only rustzigbeam_vm can depend on rustzigbeam_bif
for pkg in "${PACKAGES[@]}"; do
    if [[ "$pkg" != "rustzigbeam_vm" ]]; then
        if cargo metadata --format-version 1 2>/dev/null | jq -e ".packages[] | select(.name == \"$pkg\") | .dependencies[] | select(.name == \"rustzigbeam_bif\")" > /dev/null 2>&1; then
            echo "ERROR: $pkg depends on rustzigbeam_bif (only rustzigbeam_vm should)"
            exit 1
        fi
    fi
done

# Rule 3: Check no crate outside runtime/abi depends on VM
for pkg in "${PACKAGES[@]}"; do
    if [[ "$pkg" != "rustzigbeam_runtime" && "$pkg" != "rustzigbeam_vm" ]]; then
        if cargo metadata --format-version 1 2>/dev/null | jq -e ".packages[] | select(.name == \"$pkg\") | .dependencies[] | select(.name == \"rustzigbeam_vm\")" > /dev/null 2>&1; then
            echo "ERROR: $pkg depends on rustzigbeam_vm (only rustzigbeam_runtime should)"
            exit 1
        fi
    fi
done

echo "All dependency rules validated successfully"
exit 0