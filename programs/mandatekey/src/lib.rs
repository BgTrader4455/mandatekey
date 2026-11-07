//! MandateKey — reference implementation of the agent mandate spec v0.1 (see SPEC.md).
//!
//! STATUS: Milestone 1 work in progress. Not audited. Do not use with real funds.

use anchor_lang::prelude::*;
use anchor_spl::token_interface::{
    transfer_checked, Mint, TokenAccount, TokenInterface, TransferChecked,
};

declare_id!("11111111111111111111111111111111"); // replaced on first deploy

pub const MAX_TOKEN_LIMITS: usize = 8;
pub const MAX_DESTINATIONS: usize = 32;
pub const MAX_MEMO_LEN: usize = 64;

#[program]
pub mod mandatekey {
    use super::*;

    /// SPEC §4.1
    pub fn create_mandate(ctx: Context<CreateMandate>, args: CreateMandateArgs) -> Result<()> {
        require!(args.expires_at > args.valid_from, MandateError::InvalidWindow);
        require!(args.token_limits.len() <= MAX_TOKEN_LIMITS, MandateError::TooManyLimits);
        require!(
            args.destination_allowlist.len() <= MAX_DESTINATIONS,
            MandateError::TooManyDestinations
        );
        for l in &args.token_limits {
            require!(l.period_secs > 0, MandateError::InvalidPeriod);
            require!(l.per_payment_max <= l.period_max, MandateError::InvalidLimit);
        }

        let now = Clock::get()?.unix_timestamp;
        let m = &mut ctx.accounts.mandate;
        m.version = 1;
        m.owner = ctx.accounts.owner.key();
        m.agent = args.agent;
        m.mandate_id = args.mandate_id;
        m.cosigner = args.cosigner;
        m.valid_from = args.valid_from;
        m.expires_at = args.expires_at;
        m.revoked = false;
        m.intent_hash = args.intent_hash;
        m.token_limits = args
            .token_limits
            .into_iter()
            .map(|l| TokenLimit {
                mint: l.mint,
                per_payment_max: l.per_payment_max,
                period_max: l.period_max,
                period_secs: l.period_secs,
                period_start: now,
                spent_in_period: 0,
                cosign_threshold: l.cosign_threshold,
            })
            .collect();
        m.destination_allowlist = args.destination_allowlist;
        m.allow_any_destination = args.allow_any_destination;
        m.payment_count = 0;
        m.bump = ctx.bumps.mandate;

        emit!(MandateCreated {
            mandate: m.key(),
            owner: m.owner,
            agent: m.agent,
            expires_at: m.expires_at,
            intent_hash: m.intent_hash,
        });
        Ok(())
    }

    /// SPEC §4.2 — checks run in the order the spec defines.
    pub fn pay(ctx: Context<Pay>, amount: u64, memo: Vec<u8>) -> Result<()> {
        require!(memo.len() <= MAX_MEMO_LEN, MandateError::MemoTooLong);
        let now = Clock::get()?.unix_timestamp;
        let mint_key = ctx.accounts.mint.key();
        let recipient_owner = ctx.accounts.recipient.owner;
        let cosigned = ctx.accounts.cosigner.is_some();

        let m = &mut ctx.accounts.mandate;
        // 1. not revoked
        require!(!m.revoked, MandateError::Revoked);
        // 2. inside validity window
        require!(now >= m.valid_from && now < m.expires_at, MandateError::OutsideWindow);
        // 3. mint is listed
        let idx = m
            .token_limits
            .iter()
            .position(|l| l.mint == mint_key)
            .ok_or(MandateError::MintNotAllowed)?;
        // 4. destination allowed
        require!(
            m.allow_any_destination || m.destination_allowlist.contains(&recipient_owner),
            MandateError::DestinationNotAllowed
        );

        let cosigner_cfg = m.cosigner;
        let limit = &mut m.token_limits[idx];
        // 5. period roll-over
        let elapsed = now.checked_sub(limit.period_start).ok_or(MandateError::Overflow)?;
        if elapsed >= limit.period_secs as i64 {
            let periods = elapsed / limit.period_secs as i64;
            limit.period_start = limit
                .period_start
                .checked_add(periods * limit.period_secs as i64)
                .ok_or(MandateError::Overflow)?;
            limit.spent_in_period = 0;
        }
        // 6. per-payment cap
        require!(amount > 0 && amount <= limit.per_payment_max, MandateError::PerPaymentExceeded);
        // 7. period cap
        let new_spent = limit
            .spent_in_period
            .checked_add(amount)
            .ok_or(MandateError::Overflow)?;
        require!(new_spent <= limit.period_max, MandateError::PeriodExceeded);
        // 8. co-signer above threshold
        if amount >= limit.cosign_threshold {
            let expected = cosigner_cfg.ok_or(MandateError::CosignerRequired)?;
            let signer = ctx
                .accounts
                .cosigner
                .as_ref()
                .ok_or(MandateError::CosignerRequired)?;
            require_keys_eq!(signer.key(), expected, MandateError::CosignerRequired);
        }
        // 9. vault balance
        require!(ctx.accounts.vault.amount >= amount, MandateError::InsufficientFunds);

        limit.spent_in_period = new_spent;
        m.payment_count = m.payment_count.checked_add(1).ok_or(MandateError::Overflow)?;
        let payment_count = m.payment_count;

        let owner = m.owner;
        let agent = m.agent;
        let id_bytes = m.mandate_id.to_le_bytes();
        let bump = [m.bump];
        let seeds: &[&[u8]] = &[b"mandate", owner.as_ref(), agent.as_ref(), &id_bytes, &bump];

        transfer_checked(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                TransferChecked {
                    from: ctx.accounts.vault.to_account_info(),
                    mint: ctx.accounts.mint.to_account_info(),
                    to: ctx.accounts.recipient.to_account_info(),
                    authority: ctx.accounts.mandate.to_account_info(),
                },
                &[seeds],
            ),
            amount,
            ctx.accounts.mint.decimals,
        )?;

        emit!(PaymentMade {
            mandate: ctx.accounts.mandate.key(),
            mint: mint_key,
            amount,
            recipient: recipient_owner,
            payment_count,
            spent_in_period: new_spent,
            cosigned,
            memo,
        });
        Ok(())
    }

    /// SPEC §4.4
    pub fn revoke(ctx: Context<OwnerOnly>) -> Result<()> {
        let m = &mut ctx.accounts.mandate;
        m.revoked = true;
        emit!(MandateRevoked { mandate: m.key(), slot: Clock::get()?.slot });
        Ok(())
    }

    // TODO (M1): update_mandate (§4.3), withdraw (§4.5), close_mandate (§4.6),
    // Token-2022 extension rejection (§8.7).
}

// ---------- Accounts ----------

#[account]
#[derive(InitSpace)]
pub struct Mandate {
    pub version: u8,
    pub owner: Pubkey,
    pub agent: Pubkey,
    pub mandate_id: u64,
    pub cosigner: Option<Pubkey>,
    pub valid_from: i64,
    pub expires_at: i64,
    pub revoked: bool,
    pub intent_hash: [u8; 32],
    #[max_len(8)]
    pub token_limits: Vec<TokenLimit>,
    #[max_len(32)]
    pub destination_allowlist: Vec<Pubkey>,
    pub allow_any_destination: bool,
    pub payment_count: u64,
    pub bump: u8,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, InitSpace)]
pub struct TokenLimit {
    pub mint: Pubkey,
    pub per_payment_max: u64,
    pub period_max: u64,
    pub period_secs: u32,
    pub period_start: i64,
    pub spent_in_period: u64,
    pub cosign_threshold: u64,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct TokenLimitArgs {
    pub mint: Pubkey,
    pub per_payment_max: u64,
    pub period_max: u64,
    pub period_secs: u32,
    pub cosign_threshold: u64,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct CreateMandateArgs {
    pub agent: Pubkey,
    pub mandate_id: u64,
    pub cosigner: Option<Pubkey>,
    pub valid_from: i64,
    pub expires_at: i64,
    pub intent_hash: [u8; 32],
    pub token_limits: Vec<TokenLimitArgs>,
    pub destination_allowlist: Vec<Pubkey>,
    pub allow_any_destination: bool,
}

#[derive(Accounts)]
#[instruction(args: CreateMandateArgs)]
pub struct CreateMandate<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,
    #[account(
        init,
        payer = owner,
        space = 8 + Mandate::INIT_SPACE,
        seeds = [b"mandate", owner.key().as_ref(), args.agent.as_ref(), &args.mandate_id.to_le_bytes()],
        bump
    )]
    pub mandate: Account<'info, Mandate>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Pay<'info> {
    pub agent: Signer<'info>,
    pub cosigner: Option<Signer<'info>>,
    #[account(mut, has_one = agent)]
    pub mandate: Account<'info, Mandate>,
    pub mint: InterfaceAccount<'info, Mint>,
    #[account(
        mut,
        token::mint = mint,
        token::authority = mandate,
        token::token_program = token_program
    )]
    pub vault: InterfaceAccount<'info, TokenAccount>,
    #[account(mut, token::mint = mint, token::token_program = token_program)]
    pub recipient: InterfaceAccount<'info, TokenAccount>,
    pub token_program: Interface<'info, TokenInterface>,
}

#[derive(Accounts)]
pub struct OwnerOnly<'info> {
    pub owner: Signer<'info>,
    #[account(mut, has_one = owner)]
    pub mandate: Account<'info, Mandate>,
}

// ---------- Events ----------

#[event]
pub struct MandateCreated {
    pub mandate: Pubkey,
    pub owner: Pubkey,
    pub agent: Pubkey,
    pub expires_at: i64,
    pub intent_hash: [u8; 32],
}

#[event]
pub struct PaymentMade {
    pub mandate: Pubkey,
    pub mint: Pubkey,
    pub amount: u64,
    pub recipient: Pubkey,
    pub payment_count: u64,
    pub spent_in_period: u64,
    pub cosigned: bool,
    pub memo: Vec<u8>,
}

#[event]
pub struct MandateRevoked {
    pub mandate: Pubkey,
    pub slot: u64,
}

// ---------- Errors ----------

#[error_code]
pub enum MandateError {
    #[msg("Mandate is revoked")]
    Revoked,
    #[msg("Outside mandate validity window")]
    OutsideWindow,
    #[msg("Mint not allowed by mandate")]
    MintNotAllowed,
    #[msg("Destination not allowed by mandate")]
    DestinationNotAllowed,
    #[msg("Per-payment limit exceeded")]
    PerPaymentExceeded,
    #[msg("Period limit exceeded")]
    PeriodExceeded,
    #[msg("Co-signer signature required")]
    CosignerRequired,
    #[msg("Insufficient vault balance")]
    InsufficientFunds,
    #[msg("Invalid validity window")]
    InvalidWindow,
    #[msg("Period length must be > 0")]
    InvalidPeriod,
    #[msg("Per-payment max exceeds period max")]
    InvalidLimit,
    #[msg("Too many token limits")]
    TooManyLimits,
    #[msg("Too many destinations")]
    TooManyDestinations,
    #[msg("Memo too long")]
    MemoTooLong,
    #[msg("Arithmetic overflow")]
    Overflow,
}
