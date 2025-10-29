#!/bin/bash
set -e

NETWORK="${1:-sepolia}"
ACCOUNT="${2:-}"
VK="${3:-0x009c661e44c7e5e76f0aafdfab8ceb7c76357cc5ba5863a7dfa0b306807f8c02}"

if [ -z "$ACCOUNT" ]; then
    echo "Error: Account name required"
    echo "Usage: ./deploy.sh <network> <account-name> [verification-key]"
    exit 1
fi

echo "Building contract..."
scarb build

echo "Declaring contract..."
DECLARE_OUTPUT=$(sncast --profile "$NETWORK" --account "$ACCOUNT" declare --contract-name SettlementContract 2>&1)

if echo "$DECLARE_OUTPUT" | grep -q "is already declared"; then
    echo "Contract already declared, using existing class hash"
    CLASS_HASH=$(echo "$DECLARE_OUTPUT" | grep -o '0x[0-9a-fA-F]*' | head -1)
else
    CLASS_HASH=$(echo "$DECLARE_OUTPUT" | grep -o 'class_hash: 0x[0-9a-fA-F]*' | head -1 | sed 's/class_hash: //')
fi

if [ -z "$CLASS_HASH" ]; then
    echo "Failed to get class hash"
    echo "$DECLARE_OUTPUT"
    exit 1
fi

echo "Deploying contract..."
# Convert VK to u256 (split into low and high parts)
# Pad to 64 hex chars if needed, then split: high = first 32 chars, low = last 32 chars
VK_STRIPPED=$(echo "$VK" | sed 's/0x//')
VK_PADDED=$(printf "%064s" "$VK_STRIPPED" | tr ' ' '0')
VK_HIGH=$(echo "$VK_PADDED" | cut -c1-32)
VK_LOW=$(echo "$VK_PADDED" | cut -c33-64)
echo "VK (u256): high=0x$VK_HIGH, low=0x$VK_LOW"
DEPLOY_OUTPUT=$(sncast --profile "$NETWORK" --account "$ACCOUNT" deploy --class-hash "$CLASS_HASH" --constructor-calldata 0 0x$VK_LOW 0x$VK_HIGH 2>&1)
CONTRACT_ADDRESS=$(echo "$DEPLOY_OUTPUT" | grep -o 'contract_address: 0x[0-9a-fA-F]*' | sed 's/contract_address: //')

if [ -z "$CONTRACT_ADDRESS" ]; then
    echo "Deployment failed"
    echo "$DEPLOY_OUTPUT"
    exit 1
fi

echo ""
echo "Deployed successfully!"
echo "Contract: $CONTRACT_ADDRESS"
echo "Class:    $CLASS_HASH"
echo "Explorer: https://sepolia.starkscan.co/contract/$CONTRACT_ADDRESS"
