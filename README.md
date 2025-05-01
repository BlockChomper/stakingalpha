# Staking Prototype

A Solana program that implements a flexible staking and reward system using SPL tokens. This program allows users to stake tokens, earn time-based rewards, and withdraw both their stake and rewards.

## Table of Contents

- [Overview](#overview)
- [Architecture](#architecture)
- [Key Features](#key-features)
- [Account Structure](#account-structure)
- [Instructions](#instructions)
- [Reward Calculation](#reward-calculation)
- [Security Considerations](#security-considerations)
- [Usage Guide](#usage-guide)
- [Development](#development)

## Overview

The Staking Prototype allows users to stake SPL tokens in a centralized pool and earn rewards over time based on a configurable reward rate. The rewards are distributed from a reward pool managed by the program. The system is designed to be flexible, secure, and compatible with any SPL token.

## Architecture

The program is built on Solana using the Anchor framework. It utilizes:

- SPL Token Program for token operations
- Program Derived Addresses (PDAs) for secure account management
- Anchor's security features and account validation

### Flow Diagram

```
User                                        Program
  |                                            |
  |-- Initialize Staking Pool --------------->|
  |-- Stake Tokens -------------------------->|
  |   (Tokens transferred to pool)             |
  |                                            |
  |   (Time passes, rewards accrue)            |
  |                                            |
  |-- Unstake Tokens ------------------------>|
  |   (Tokens returned to user)                |
  |                                            |
  |-- Claim Rewards ------------------------->|
  |   (Reward tokens transferred to user)      |
  |                                            |
```

## Key Features

- **Token Staking**: Users can stake any SPL token
- **Time-based Rewards**: Rewards accrue based on stake amount, time, and reward rate
- **Flexible Reward Rate**: Admin can adjust the reward rate
- **Secure Token Transfers**: All token operations use secure Solana CPI calls
- **Pro-rated Rewards**: Rewards are calculated down to the second

## Account Structure

### StakingPool

The main account that tracks global staking information:

- `admin`: The authority controlling the staking pool
- `reward_rate`: Tokens rewarded per day per staked token (multiplier)
- `total_staked`: Total amount of tokens staked across all users
- `last_update_time`: Unix timestamp of the last update
- `stake_mint`: The mint address of the token being staked
- `reward_mint`: The mint address of the token given as rewards
- `pool_stake_account`: Token account holding staked tokens
- `pool_reward_account`: Token account holding reward tokens

### UserStake

Per-user account that tracks individual staking information:

- `owner`: The user's wallet address
- `stake_amount`: Amount of tokens staked by this user
- `reward_debt`: Accumulated rewards pending collection
- `last_stake_time`: Last time the user staked/unstaked/claimed

## Instructions

### 1. Initialize

Creates and initializes a new staking pool:

```rust
pub fn initialize(
    ctx: Context<Initialize>,
    reward_rate: u64,
) -> Result<()>
```

- `reward_rate`: Number of reward tokens to distribute per day per staked token

### 2. Stake

Stakes tokens into the pool:

```rust
pub fn stake(ctx: Context<Stake>, amount: u64) -> Result<()>
```

- `amount`: Number of tokens to stake

### 3. Unstake

Withdraws staked tokens from the pool:

```rust
pub fn unstake(ctx: Context<Unstake>, amount: u64) -> Result<()>
```

- `amount`: Number of tokens to unstake

### 4. Claim Rewards

Collects accrued rewards:

```rust
pub fn claim_rewards(ctx: Context<ClaimRewards>) -> Result<()>
```

### 5. Update Reward Rate

Modifies the reward distribution rate (admin only):

```rust
pub fn update_reward_rate(ctx: Context<UpdateRewardRate>, new_rate: u64) -> Result<()>
```

- `new_rate`: New reward rate to set

## Reward Calculation

Rewards are calculated based on the formula:

```
reward = stake_amount * reward_rate * time_staked
```

Where:
- `stake_amount` is the number of tokens staked
- `reward_rate` is tokens per day per staked token
- `time_staked` is measured in days (with partial days pro-rated to the second)

The implementation uses checked arithmetic to prevent overflows:

```rust
fn calculate_pending_reward(stake_amount: u64, reward_rate: u64, time_passed: i64) -> Result<u64> {
    if time_passed <= 0 || stake_amount == 0 {
        return Ok(0);
    }

    // Convert time_passed to days and seconds
    let days = time_passed.checked_div(86400).unwrap_or(0) as u64;
    let remainder_seconds = time_passed.checked_rem(86400).unwrap_or(0) as u64;
    
    // Calculate full days of rewards
    let mut reward = stake_amount
        .checked_mul(reward_rate)
        .ok_or(ErrorCode::ArithmeticError)?
        .checked_mul(days)
        .ok_or(ErrorCode::ArithmeticError)?;

    // Add partial day rewards (pro-rated)
    if remainder_seconds > 0 {
        let partial_reward = stake_amount
            .checked_mul(reward_rate)
            .ok_or(ErrorCode::ArithmeticError)?
            .checked_mul(remainder_seconds)
            .ok_or(ErrorCode::ArithmeticError)?
            .checked_div(86400)
            .ok_or(ErrorCode::ArithmeticError)?;
        
        reward = reward.checked_add(partial_reward).ok_or(ErrorCode::ArithmeticError)?;
    }

    Ok(reward)
}
```

## Security Considerations

1. **Reentrancy Protection**: All state changes happen before external calls
2. **Arithmetic Safety**: All calculations use checked math to prevent overflows
3. **Authority Checks**: Only the admin can update reward rates
4. **PDA Validation**: Token accounts are properly validated with constraints
5. **Proper Signing**: PDA signing for token transfers from pool accounts

## Usage Guide

### For Pool Administrators

1. **Initialize the Pool**:
   - Create the stake and reward token mints
   - Create pool token accounts
   - Call `initialize` with desired reward rate

2. **Fund the Reward Pool**:
   - Transfer reward tokens to the pool's reward account
   
3. **Manage Rewards**:
   - Monitor pool activity
   - Adjust reward rate as needed using `update_reward_rate`

### For Users

1. **Stake Tokens**:
   - Obtain stake tokens
   - Approve token transfer
   - Call `stake` with desired amount

2. **Collect Rewards**:
   - Periodically call `claim_rewards` to collect accumulated rewards
   - Rewards are transferred to your token account

3. **Unstake Tokens**:
   - Call `unstake` when you want to withdraw
   - Specify amount to unstake
   - Tokens are returned to your token account

## Development

### Prerequisites

- Rust
- Solana CLI
- Anchor Framework
- Yarn/NPM

### Building

```bash
anchor build
```

### Testing

```bash
anchor test
```

### Deployment

```bash
anchor deploy
```

## Error Codes

- `InsufficientStakeAmount`: Attempted to unstake more than was staked
- `ArithmeticError`: Math operation failed (likely overflow/underflow)
- `Unauthorized`: Operation requires admin privileges
- `NoRewardsToClaim`: No rewards available to claim

---

## License

This project is open-source and available under the MIT License.

## Disclaimer

This is a prototype staking system and should be thoroughly audited before being used in production environments. 