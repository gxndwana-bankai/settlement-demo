#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/program"

anchor build
solana config set -ud
solana program deploy target/deploy/settlement_solana_program.so


