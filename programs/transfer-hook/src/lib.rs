use anchor_lang::{
    prelude::*,
    system_program::{create_account, CreateAccount},
};
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{transfer_checked, Mint, TokenAccount, TokenInterface, TransferChecked, Token2022},
};
use spl_tlv_account_resolution::{
    account::ExtraAccountMeta, seeds::Seed, state::ExtraAccountMetaList,
};
use spl_transfer_hook_interface::instruction::{ExecuteInstruction, TransferHookInstruction};
use anchor_spl::token::Token;

declare_id!("9xqzzFzHEBYzDPH4gfiZF29xma8uksQZA1LgAybhq5G7");
 
#[program]
pub mod transfer_hook {
    use super::*;
 
    pub fn initialize_extra_account_meta_list(
        ctx: Context<InitializeExtraAccountMetaList>,
    ) -> Result<()> {
        // index 0-3 are the accounts required for token transfer (source, mint, destination, owner)
        // index 4 is address of ExtraAccountMetaList account
        // The `addExtraAccountsToInstruction` JS helper function resolving incorrectly
        let account_metas = vec![
            // index 5, wrapped SOL mint
            ExtraAccountMeta::new_with_pubkey(&ctx.accounts.wsol_mint.key(), false, false)?,
            // index 6, token program
            ExtraAccountMeta::new_with_pubkey(&ctx.accounts.token_program.key(), false, false)?,
            // index 7, associated token program
            ExtraAccountMeta::new_with_pubkey(
                &ctx.accounts.associated_token_program.key(),
                false,
                false,
            )?,
            // index 8, delegate PDA
            ExtraAccountMeta::new_with_seeds(
                &[Seed::Literal {
                    bytes: "delegate".as_bytes().to_vec(),
                }],
                false, // is_signer
                false,  // is_writable
            )?,
            // index 9, delegate wrapped SOL token account
            ExtraAccountMeta::new_external_pda_with_seeds(
                7, // associated token program index
                &[
                    Seed::AccountKey { index: 8 }, // owner index (delegate PDA)
                    Seed::AccountKey { index: 6 }, // token program index
                    Seed::AccountKey { index: 5 }, // wsol mint index
                ],
                false, // is_signer
                true,  // is_writable
            )?,
            // index 10, sender wrapped SOL token account
            ExtraAccountMeta::new_external_pda_with_seeds(
                7, // associated token program index
                &[
                    Seed::AccountKey { index: 3 }, // owner index
                    Seed::AccountKey { index: 6 }, // token program index
                    Seed::AccountKey { index: 5 }, // wsol mint index
                ],
                false, // is_signer
                true,  // is_writable
            )?,
        ];
 
        // calculate account size
        let account_size = ExtraAccountMetaList::size_of(account_metas.len())? as u64;
        // calculate minimum required lamports
        let lamports = Rent::get()?.minimum_balance(account_size as usize);
 
        let mint = ctx.accounts.mint.key();
        let signer_seeds: &[&[&[u8]]] = &[&[
            b"extra-account-metas",
            &mint.as_ref(),
            &[ctx.bumps.extra_account_meta_list],
        ]];
 
        // create ExtraAccountMetaList account
        create_account(
            CpiContext::new(
                ctx.accounts.system_program.to_account_info(),
                CreateAccount {
                    from: ctx.accounts.payer.to_account_info(),
                    to: ctx.accounts.extra_account_meta_list.to_account_info(),
                },
            )
            .with_signer(signer_seeds),
            lamports,
            account_size,
            ctx.program_id,
        )?;
 
        // initialize ExtraAccountMetaList account with extra accounts
        ExtraAccountMetaList::init::<ExecuteInstruction>(
            &mut ctx.accounts.extra_account_meta_list.try_borrow_mut_data()?,
            &account_metas,
        )?;
 
        Ok(())
    }
 
    // Require SOL fee on transfer, lamport fee is equal to transfer amount
    // If this fails, the initial token transfer fails
    pub fn transfer_hook(ctx: Context<TransferHook>, amount: u64) -> Result<()> {
        msg!("Transfer WSOL using delegate PDA");
    
        let signer_seeds: &[&[&[u8]]] = &[&[b"delegate", &[ctx.bumps.delegate]]];
    
        // transfer WSOL from sender to delegate token account using delegate PDA
        transfer_checked(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                TransferChecked {
                    from: ctx.accounts.sender_wsol_token_account.to_account_info(),
                    mint: ctx.accounts.wsol_mint.to_account_info(),
                    to: ctx.accounts.delegate_wsol_token_account.to_account_info(),
                    authority: ctx.accounts.delegate.to_account_info(),
                },
            )
            .with_signer(signer_seeds),
            amount,
            ctx.accounts.wsol_mint.decimals,
        )?;
        Ok(())
    }

    pub fn claim(ctx: Context<ClaimTokens>, amount: u64) -> Result<()> {
        // require!(ctx.accounts.vesting_account.claimed == false, VestingErr::AlreadyClaimed);
        let signer_seeds: &[&[&[u8]]] = &[&[b"delegate", &[ctx.bumps.delegate]]];

        // transfer WSOL from sender to delegate token account using delegate PDA
        transfer_checked(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                TransferChecked {
                    from: ctx.accounts.delegate_wsol_token_account.to_account_info(),
                    mint: ctx.accounts.wsol_mint.to_account_info(),
                    to: ctx.accounts.receive_wsol_token_account.to_account_info(),
                    authority: ctx.accounts.delegate.to_account_info(),
                },
            )
            .with_signer(signer_seeds),
            amount,
            ctx.accounts.wsol_mint.decimals,
        )?;

        Ok(())
    }
 
    // fallback instruction handler as workaround to anchor instruction discriminator check
    pub fn fallback<'info>(
        program_id: &Pubkey,
        accounts: &'info [AccountInfo<'info>],
        data: &[u8],
    ) -> Result<()> {
        let instruction = TransferHookInstruction::unpack(data)?;
    
        // match instruction discriminator to transfer hook interface execute instruction
        // token2022 program CPIs this instruction on token transfer
        match instruction {
            TransferHookInstruction::Execute { amount } => {
                let amount_bytes = amount.to_le_bytes();
    
                // invoke custom transfer hook instruction on our program
                __private::__global::transfer_hook(program_id, accounts, &amount_bytes)
            }
            _ => return Err(ProgramError::InvalidInstructionData.into()),
        }
    }

    
}
 
#[derive(Accounts)]
pub struct InitializeExtraAccountMetaList<'info> {
    #[account(mut)]
    payer: Signer<'info>,
 
    /// CHECK: ExtraAccountMetaList Account, must use these seeds
    #[account(
        mut,
        seeds = [b"extra-account-metas", mint.key().as_ref()],
        bump
    )]
    pub extra_account_meta_list: AccountInfo<'info>,
    pub mint: InterfaceAccount<'info, Mint>,
    pub wsol_mint: InterfaceAccount<'info, Mint>,
    pub token_program: Interface<'info, TokenInterface>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

// Order of accounts matters for this struct.
// The first 4 accounts are the accounts required for token transfer (source, mint, destination, owner)
// Remaining accounts are the extra accounts required from the ExtraAccountMetaList account
// These accounts are provided via CPI to this program from the token2022 program
#[derive(Accounts)]
pub struct TransferHook<'info> {
    #[account(
        token::mint = mint,
        token::authority = owner,
    )]
    pub source_token: InterfaceAccount<'info, TokenAccount>,
    pub mint: InterfaceAccount<'info, Mint>,
    #[account(
        token::mint = mint,
    )]
    pub destination_token: InterfaceAccount<'info, TokenAccount>,
    /// CHECK: source token account owner, can be SystemAccount or PDA owned by another program
    pub owner: UncheckedAccount<'info>,
    /// CHECK: ExtraAccountMetaList Account,
    #[account(
        seeds = [b"extra-account-metas", mint.key().as_ref()],
        bump
    )]
    pub extra_account_meta_list: UncheckedAccount<'info>,
    pub wsol_mint: InterfaceAccount<'info, Mint>,
    pub token_program: Interface<'info, TokenInterface>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    #[account(
        seeds = [b"delegate"],
        bump
    )]
    pub delegate: SystemAccount<'info>,
    #[account(
        mut,
        token::mint = wsol_mint,
        token::authority = delegate,
    )]
    pub delegate_wsol_token_account: InterfaceAccount<'info, TokenAccount>,
    #[account(
        mut,
        token::mint = wsol_mint,
        token::authority = owner,
    )]
    pub sender_wsol_token_account: InterfaceAccount<'info, TokenAccount>,
}

#[derive(Accounts)]
pub struct ClaimTokens<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    #[account(
        seeds = [b"delegate"],
        bump
    )]
    pub delegate: SystemAccount<'info>,

    #[account(mut)]
    pub wsol_mint: InterfaceAccount<'info, Mint>,

    #[account(
        mut,
        token::mint = wsol_mint,
        token::authority = delegate,
    )]
    pub delegate_wsol_token_account: InterfaceAccount<'info, TokenAccount>,
    #[account(
        mut,
        token::mint = wsol_mint,
        token::authority = user,
    )]
    pub receive_wsol_token_account: InterfaceAccount<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}   