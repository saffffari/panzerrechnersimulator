#!/usr/bin/env bash
set -euo pipefail

kessel_repo=/home/alex/tank_simulator
cd "$kessel_repo"

/home/alex/.cargo/bin/cargo build --release
exec "$kessel_repo/target/release/kessel"
