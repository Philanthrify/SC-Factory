# Philanthrify Architecture

This repo contains smart contracts for Philanthrify, a decentralized donation platform using a factory-template system.

## Overview

There are three main components:

1. Philanthrify Factory
2. Charity Contract
3. Project Contract

### Philanthrify Factory

- Deploys Charity contracts using the Charity Template
- Does not accept donations or mint NFTs

### Charity Contract

- Deployed by the factory
- Deploys Project contracts using the Project Template
- Accepts donations via the donate endpoint
  - Mints an NFT receipt to the donor's wallet
- Can donate to Project contracts via donateToProject

### Project Contract

- Can only receive funds from its parent Charity contract
- Direct donations will fail
- Deployed via deployProject from the Charity contract
- mints an nft and sends to charity contract

## Notes

- Only the Charity and Project contracts mints NFTs
- Factory is only for deploying contracts
