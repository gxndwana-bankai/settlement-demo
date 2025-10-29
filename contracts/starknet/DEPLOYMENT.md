# Starknet Settlement Contract Deployment Guide

This guide will help you deploy the Settlement Contract to Starknet Sepolia testnet.

## Prerequisites

1. **Starknet Foundry** installed (includes `sncast` and `snforge`)
   ```bash
   curl -L https://raw.githubusercontent.com/foundry-rs/starknet-foundry/master/scripts/install.sh | sh
   snfoundryup
   ```

2. **Scarb** installed (Starknet's package manager)
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://docs.swmansion.com/scarb/install.sh | sh
   ```

## Setup

### Step 1: Create a Starknet Account

If you don't have a Starknet account yet, create one:

```bash
# Create account
sncast account create \
  --url https://starknet-sepolia.public.blastapi.io/rpc/v0_7 \
  --name my_account
```

This will output your account address and private key. **Save these securely!**

### Step 2: Fund Your Account

Get testnet ETH from the Starknet faucet:
- Visit: https://starknet-faucet.vercel.app/
- Enter your account address
- Request testnet ETH

### Step 3: Deploy Your Account

Before you can use your account, you need to deploy it:

```bash
sncast account deploy \
  --name my_account \
  --url https://starknet-sepolia.public.blastapi.io/rpc/v0_7 \
  --max-fee 0.01
```

## Deployment

Now you're ready to deploy the Settlement Contract!

### Basic Deployment

```bash
cd contracts/starknet
./deploy.sh sepolia my_account
```

### Custom Verification Key

If you have a specific verification key (VK) to use:

```bash
./deploy.sh sepolia my_account 0xYOUR_VK_HERE
```

The default VK is: `0x00fdf0c1e13611d90ea75235695fc7f99dde2c530e4f67d0e4c9ab6a08a1be2ac5`

## What the Script Does

1. **Builds** the contract using `scarb build`
2. **Declares** the contract class on Starknet (if not already declared)
3. **Deploys** a new instance with the specified verification key
4. **Saves** deployment info to `deployments/sepolia_deployment.txt`

## After Deployment

Once deployed, you'll receive:
- **Class Hash**: The hash of your contract class
- **Contract Address**: The address of your deployed contract
- **Starkscan Link**: Direct link to view your contract on the block explorer

### Interact with Your Contract

You can interact with your deployed contract using `sncast`:

```bash
# Get the verification key
sncast call \
  --url https://starknet-sepolia.public.blastapi.io/rpc/v0_7 \
  --contract-address YOUR_CONTRACT_ADDRESS \
  --function get_vk

# Submit an order
sncast invoke \
  --url https://starknet-sepolia.public.blastapi.io/rpc/v0_7 \
  --contract-address YOUR_CONTRACT_ADDRESS \
  --function submit_order \
  --account my_account \
  --calldata 1 2 0x123 1000 100
```

## Troubleshooting

### "Account not found"
Make sure you've created and deployed your account (Steps 1-3).

### "Insufficient funds"
Get more testnet ETH from the faucet: https://starknet-faucet.vercel.app/

### "Class already declared"
This is normal! The script will use the existing class hash and continue with deployment.

### "Invalid constructor calldata"
Check that your verification key is a valid u256 hex value.

## Mainnet Deployment

⚠️ **Warning**: Deploying to mainnet requires real ETH!

To deploy to mainnet:
1. Update your account with mainnet ETH
2. Use the mainnet profile:
   ```bash
   ./deploy.sh mainnet my_account YOUR_VK
   ```

## Support

For issues with:
- **Starknet Foundry**: https://github.com/foundry-rs/starknet-foundry
- **Scarb**: https://github.com/software-mansion/scarb
- **Starknet**: https://docs.starknet.io/

