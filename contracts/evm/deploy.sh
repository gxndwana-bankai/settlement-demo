#!/bin/bash

# Load environment variables
source .env

# Deploy to Base Sepolia (Chain ID: 84532)
echo "Deploying to Base Sepolia (Chain ID: 84532)..."
forge script script/Deploy.s.sol:DeploySettlement \
    --rpc-url $BASE_SEPOLIA_RPC \
    --broadcast \
    --verify \
    --verifier etherscan \
    --etherscan-api-key $ETHERSCAN_API_KEY \
    --chain 84532

echo ""
echo "Deployment to Base Sepolia complete!"
echo ""

# Deploy to Arbitrum Sepolia (Chain ID: 421614)
echo "Deploying to Arbitrum Sepolia (Chain ID: 421614)..."
forge script script/Deploy.s.sol:DeploySettlement \
    --rpc-url $ARB_SEPOLIA_RPC \
    --broadcast \
    --verify \
    --verifier etherscan \
    --etherscan-api-key $ETHERSCAN_API_KEY \
    --chain 421614

echo ""
echo "Deployment to Arbitrum Sepolia complete!"

