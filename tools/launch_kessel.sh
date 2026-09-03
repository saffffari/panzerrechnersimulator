#!/usr/bin/env bash
set -euo pipefail

kessel_repo="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$kessel_repo"

cargo build --release
exec "$kessel_repo/target/release/kessel"
