use anchor_lang::prelude::*;
use anchor_lang::solana_program::clock::Clock;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer};
use anchor_spl::associated_token::AssociatedToken;

declare_id!("A6wFmzoTbvudsizcaC8YrrfsuQJD8qf1WHvj1bv2y76u");

#[program]
pub mod stakingprototype {
    use super::*;

    pub fn initialize(
        ctx: Context<Initialize>,
        reward_rate: u64,
    ) -> Result<()> {
        let staking_pool = &mut ctx.accounts.staking_pool;
        let admin = &ctx.accounts.admin;

        staking_pool.admin = admin.key();
        staking_pool.reward_rate = reward_rate;
        staking_pool.total_staked = 0;
        staking_pool.last_update_time = Clock::get()?.unix_timestamp;
        staking_pool.stake_mint = ctx.accounts.stake_mint.key();
        staking_pool.reward_mint = ctx.accounts.reward_mint.key();
        staking_pool.pool_stake_account = ctx.accounts.pool_stake_account.key();
        staking_pool.pool_reward_account = ctx.accounts.pool_reward_account.key();

        msg!("Staking pool initialized with rate: {}", reward_rate);
        Ok(())
    }

    pub fn stake(ctx: Context<Stake>, amount: u64) -> Result<()> {
        let staking_pool = &mut ctx.accounts.staking_pool;
        let user_stake = &mut ctx.accounts.user_stake;
        let user = &ctx.accounts.user;
        let clock = Clock::get()?;

        // Update rewards for the pool before changes
        let time_passed = clock.unix_timestamp - staking_pool.last_update_time;
        if time_passed > 0 && staking_pool.total_staked > 0 {
            // Update global state
            staking_pool.last_update_time = clock.unix_timestamp;
        }

        // Initialize user stake if this is their first time
        if user_stake.owner == Pubkey::default() {
            user_stake.owner = user.key();
            user_stake.stake_amount = 0;
            user_stake.reward_debt = 0;
            user_stake.last_stake_time = clock.unix_timestamp;
        } else {
            // Calculate pending rewards before updating stake
            let pending_reward = calculate_pending_reward(
                user_stake.stake_amount,
                staking_pool.reward_rate,
                clock.unix_timestamp - user_stake.last_stake_time,
            )?;
            
            user_stake.reward_debt += pending_reward;
        }

        // Transfer tokens from user to pool
        let cpi_accounts = Transfer {
            from: ctx.accounts.user_token_account.to_account_info(),
            to: ctx.accounts.pool_stake_account.to_account_info(),
            authority: user.to_account_info(),
        };
        
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);
        
        token::transfer(cpi_ctx, amount)?;

        // Update stake amount
        user_stake.stake_amount = user_stake.stake_amount.checked_add(amount).ok_or(ErrorCode::ArithmeticError)?;
        user_stake.last_stake_time = clock.unix_timestamp;
        
        // Update total staked in pool
        staking_pool.total_staked = staking_pool.total_staked.checked_add(amount).ok_or(ErrorCode::ArithmeticError)?;

        msg!("Staked {} tokens", amount);
        Ok(())
    }

    pub fn unstake(ctx: Context<Unstake>, amount: u64) -> Result<()> {
        // Get information before mutating staking_pool
        let pool_stake_account_info = ctx.accounts.pool_stake_account.to_account_info();
        let user_token_account_info = ctx.accounts.user_token_account.to_account_info();
        let staking_pool_info = ctx.accounts.staking_pool.to_account_info();
        let token_program_info = ctx.accounts.token_program.to_account_info();
        let bump = ctx.bumps.staking_pool;
        
        let staking_pool = &mut ctx.accounts.staking_pool;
        let user_stake = &mut ctx.accounts.user_stake;
        let clock = Clock::get()?;

        require!(
            user_stake.stake_amount >= amount,
            ErrorCode::InsufficientStakeAmount
        );

        // Calculate pending rewards before unstaking
        let pending_reward = calculate_pending_reward(
            user_stake.stake_amount,
            staking_pool.reward_rate,
            clock.unix_timestamp - user_stake.last_stake_time,
        )?;
        
        user_stake.reward_debt += pending_reward;
        
        // Update stake amount
        user_stake.stake_amount = user_stake.stake_amount.checked_sub(amount).ok_or(ErrorCode::ArithmeticError)?;
        user_stake.last_stake_time = clock.unix_timestamp;
        
        // Update total staked in pool
        staking_pool.total_staked = staking_pool.total_staked.checked_sub(amount).ok_or(ErrorCode::ArithmeticError)?;
        
        // Transfer tokens from pool to user
        let pool_signer_seeds = &[
            b"staking_pool".as_ref(),
            &[bump],
        ];
        let signer = &[&pool_signer_seeds[..]];
        
        let cpi_accounts = Transfer {
            from: pool_stake_account_info,
            to: user_token_account_info,
            authority: staking_pool_info,
        };
        
        token::transfer(
            CpiContext::new_with_signer(token_program_info, cpi_accounts, signer),
            amount
        )?;

        msg!("Unstaked {} tokens", amount);
        Ok(())
    }

    pub fn claim_rewards(ctx: Context<ClaimRewards>) -> Result<()> {
        // Get information before mutating staking_pool
        let pool_reward_account_info = ctx.accounts.pool_reward_account.to_account_info();
        let user_reward_account_info = ctx.accounts.user_reward_account.to_account_info();
        let staking_pool_info = ctx.accounts.staking_pool.to_account_info();
        let token_program_info = ctx.accounts.token_program.to_account_info();
        let bump = ctx.bumps.staking_pool;
        
        let staking_pool = &mut ctx.accounts.staking_pool;
        let user_stake = &mut ctx.accounts.user_stake;
        let clock = Clock::get()?;

        // Calculate pending rewards
        let pending_reward = calculate_pending_reward(
            user_stake.stake_amount,
            staking_pool.reward_rate,
            clock.unix_timestamp - user_stake.last_stake_time,
        )?;
        
        let total_reward = user_stake.reward_debt.checked_add(pending_reward).ok_or(ErrorCode::ArithmeticError)?;
        
        require!(total_reward > 0, ErrorCode::NoRewardsToClaim);
        
        // Reset reward debt
        user_stake.reward_debt = 0;
        user_stake.last_stake_time = clock.unix_timestamp;
        
        // Transfer reward tokens from pool to user
        let pool_signer_seeds = &[
            b"staking_pool".as_ref(),
            &[bump],
        ];
        let signer = &[&pool_signer_seeds[..]];
        
        let cpi_accounts = Transfer {
            from: pool_reward_account_info,
            to: user_reward_account_info,
            authority: staking_pool_info,
        };
        
        token::transfer(
            CpiContext::new_with_signer(token_program_info, cpi_accounts, signer),
            total_reward
        )?;

        msg!("Claimed {} reward tokens", total_reward);
        Ok(())
    }

    pub fn update_reward_rate(ctx: Context<UpdateRewardRate>, new_rate: u64) -> Result<()> {
        let staking_pool = &mut ctx.accounts.staking_pool;
        let admin = &ctx.accounts.admin;

        require!(
            admin.key() == staking_pool.admin,
            ErrorCode::Unauthorized
        );

        staking_pool.reward_rate = new_rate;
        msg!("Updated reward rate to {}", new_rate);
        Ok(())
    }
}

fn calculate_pending_reward(stake_amount: u64, reward_rate: u64, time_passed: i64) -> Result<u64> {
    if time_passed <= 0 || stake_amount == 0 {
        return Ok(0);
    }

    // Convert time_passed to seconds in a day (86400 seconds in a day)
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

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(
        init,
        payer = admin, 
        space = 8 + StakingPool::SIZE,
        seeds = [b"staking_pool"],
        bump
    )]
    pub staking_pool: Account<'info, StakingPool>,
    
    #[account(mut)]
    pub admin: Signer<'info>,
    
    pub stake_mint: Account<'info, Mint>,
    pub reward_mint: Account<'info, Mint>,
    
    #[account(
        mut,
        constraint = pool_stake_account.mint == stake_mint.key(),
        constraint = pool_stake_account.owner == staking_pool.key()
    )]
    pub pool_stake_account: Account<'info, TokenAccount>,
    
    #[account(
        mut,
        constraint = pool_reward_account.mint == reward_mint.key(),
        constraint = pool_reward_account.owner == staking_pool.key()
    )]
    pub pool_reward_account: Account<'info, TokenAccount>,
    
    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
pub struct Stake<'info> {
    #[account(
        mut,
        seeds = [b"staking_pool"],
        bump
    )]
    pub staking_pool: Account<'info, StakingPool>,
    
    #[account(
        init_if_needed,
        payer = user,
        seeds = [b"user-stake", user.key().as_ref()],
        bump,
        space = 8 + UserStake::SIZE
    )]
    pub user_stake: Account<'info, UserStake>,
    
    #[account(mut)]
    pub user: Signer<'info>,
    
    #[account(
        mut,
        constraint = user_token_account.mint == staking_pool.stake_mint,
        constraint = user_token_account.owner == user.key()
    )]
    pub user_token_account: Account<'info, TokenAccount>,
    
    #[account(
        mut,
        constraint = pool_stake_account.mint == staking_pool.stake_mint,
        constraint = pool_stake_account.key() == staking_pool.pool_stake_account
    )]
    pub pool_stake_account: Account<'info, TokenAccount>,
    
    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
pub struct Unstake<'info> {
    #[account(
        mut,
        seeds = [b"staking_pool"],
        bump
    )]
    pub staking_pool: Account<'info, StakingPool>,
    
    #[account(
        mut,
        seeds = [b"user-stake", user.key().as_ref()],
        bump,
        constraint = user_stake.owner == user.key() @ ErrorCode::Unauthorized
    )]
    pub user_stake: Account<'info, UserStake>,
    
    #[account(mut)]
    pub user: Signer<'info>,
    
    #[account(
        mut,
        constraint = user_token_account.mint == staking_pool.stake_mint,
        constraint = user_token_account.owner == user.key()
    )]
    pub user_token_account: Account<'info, TokenAccount>,
    
    #[account(
        mut,
        constraint = pool_stake_account.mint == staking_pool.stake_mint,
        constraint = pool_stake_account.key() == staking_pool.pool_stake_account
    )]
    pub pool_stake_account: Account<'info, TokenAccount>,
    
    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct ClaimRewards<'info> {
    #[account(
        mut,
        seeds = [b"staking_pool"],
        bump
    )]
    pub staking_pool: Account<'info, StakingPool>,
    
    #[account(
        mut,
        seeds = [b"user-stake", user.key().as_ref()],
        bump,
        constraint = user_stake.owner == user.key() @ ErrorCode::Unauthorized
    )]
    pub user_stake: Account<'info, UserStake>,
    
    #[account(mut)]
    pub user: Signer<'info>,
    
    #[account(
        mut,
        constraint = user_reward_account.mint == staking_pool.reward_mint,
        constraint = user_reward_account.owner == user.key()
    )]
    pub user_reward_account: Account<'info, TokenAccount>,
    
    #[account(
        mut,
        constraint = pool_reward_account.mint == staking_pool.reward_mint,
        constraint = pool_reward_account.key() == staking_pool.pool_reward_account
    )]
    pub pool_reward_account: Account<'info, TokenAccount>,
    
    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct UpdateRewardRate<'info> {
    #[account(
        mut,
        seeds = [b"staking_pool"],
        bump
    )]
    pub staking_pool: Account<'info, StakingPool>,
    pub admin: Signer<'info>,
}

#[account]
pub struct StakingPool {
    pub admin: Pubkey,
    pub reward_rate: u64,
    pub total_staked: u64,
    pub last_update_time: i64,
    pub stake_mint: Pubkey,
    pub reward_mint: Pubkey,
    pub pool_stake_account: Pubkey,
    pub pool_reward_account: Pubkey,
}

impl StakingPool {
    pub const SIZE: usize = 32 + 8 + 8 + 8 + 32 + 32 + 32 + 32;
}

#[account]
pub struct UserStake {
    pub owner: Pubkey,
    pub stake_amount: u64,
    pub reward_debt: u64,
    pub last_stake_time: i64,
}

impl UserStake {
    pub const SIZE: usize = 32 + 8 + 8 + 8;
}

#[error_code]
pub enum ErrorCode {
    #[msg("Insufficient stake amount")]
    InsufficientStakeAmount,
    #[msg("Arithmetic error")]
    ArithmeticError,
    #[msg("Unauthorized")]
    Unauthorized,
    #[msg("No rewards to claim")]
    NoRewardsToClaim,
}
