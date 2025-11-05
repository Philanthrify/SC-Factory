# Philanthrify Smart Contracts

A decentralized donation platform built on MultiversX that uses a factory-template pattern to deploy charity and project contracts with NFT receipt functionality.

## 🏗️ Architecture

The system consists of three main smart contracts:

### 1. **Factory Contract** 
- Central hub that deploys charity contracts
- Issues and manages the global NFT collection (PHILXY)
- Mints NFT receipts for all donations across the platform
- Tracks donor profiles and donation history
- Maintains global platform statistics

### 2. **Charity Contract** 
- Deployed by the factory for each charity organization
- Accepts direct donations to the charity
- Can deploy project contracts under the charity
- Forwards donations to specific projects
- Supports batch donations for multiple NFT mints

### 3. **Project Contract** 
- Deployed by charity contracts for specific fundraising projects
- Accepts donations for individual projects
- Can only receive funds through proper donation flow
- Direct transfers will fail (security feature)
- Supports batch donations


## 🔄 Donation Flow

```
Donor → Charity Contract → Factory (mints NFT) → NFT sent to Donor
     ↓
Donor → Charity → Project → Factory (mints NFT) → NFT sent to Donor
```

## 🔐 Security Features

- Owner-only functions for critical operations
- Template validation before deployment
- Upgradeable contracts 

