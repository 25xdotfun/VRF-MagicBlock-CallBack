//! A player buys `count` cells in one open lobby, all pointing at the
//! same `token_mint`. Debits `count * lobby.ticket_price` lamports
//! from the buyer to the per-lobby vault PDA and appends `count`
//! cells to `lobby.cells`. One ix call covers the whole basket — the
//! UI counter (1/3/5/max) maps directly onto a single `buy_ticket`
//! call with the matching `count`.
//!
//! When THIS buy fills the lobby (`sold == total_cells` after the
//! pushes), the same ix also fires a VRF randomness request through
//! `ephemeral_vrf_sdk`. The oracle's callback lands later in a
//! separate tx that invokes `close_lobby` (= the VRF callback). The
//! oracle_queue + vrf accounts injected by `#[vrf]` are passed on
//! every buy even though they're only consumed on the filling buy —
//! Anchor's Accounts struct is shape-stable per ix, no conditional
//! accounts.

use anchor_lang::prelude::*;
use anchor_lang::system_program;
use ephemeral_vrf_sdk::anchor::vrf;
use ephemeral_vrf_sdk::instructions::{create_request_randomness_ix, RequestRandomnessParams};
use ephemeral_vrf_sdk::types::SerializableAccountMeta;

use crate::constants::*;
use crate::errors::*;
use crate::events::TicketBought;
use crate::instruction::CloseLobby as CloseLobbyDisc;
use crate::state::*;

#[vrf]
#[derive(Accounts)]
pub struct BuyTicket<'info>
{
    #[account(
        mut,
        seeds = [LOBBY_SEED, lobby.id.to_le_bytes().as_ref()],
        bump = lobby.bump,
    )]
    pub lobby: Account<'info, Lobby>,

    /// CHECK: SOL-only vault PDA. Validated by `seeds + bump` against
    /// `lobby.vault_bump` recorded at create time.
    #[account(
        mut,
        seeds = [VAULT_SEED, lobby.key().as_ref()],
        bump = lobby.vault_bump,
    )]
    pub vault: UncheckedAccount<'info>,

    #[account(mut)]
    pub buyer: Signer<'info>,

    /// CHECK: VRF oracle queue, pinned to `ephemeral_vrf_sdk`'s
    /// `DEFAULT_QUEUE`. Only touched when the lobby fills.
    #[account(mut, address = ephemeral_vrf_sdk::consts::DEFAULT_QUEUE)]
    pub oracle_queue: AccountInfo<'info>,

    pub system_program: Program<'info, System>,
}

pub fn handler(ctx: Context<BuyTicket>, token_mint: Pubkey, count: u16) -> Result<()>
{
    let lobby = &mut ctx.accounts.lobby;

    require!(lobby.status == LobbyStatus::Open, CustomError::LobbyNotOpen);
    require!(count > 0, CustomError::InvalidTicketCount);
    require!(
        (lobby.sold as u16)
            .checked_add(count)
            .ok_or(error!(CustomError::Overflow))?
            <= lobby.total_cells as u16,
        CustomError::LobbySoldOut,
    );

    // Pull `count * ticket_price` lamports from the buyer → vault in
    // one CPI. `system_program::transfer` is signer-driven, so the
    // outer `buyer: Signer` is what authorizes it.
    let total_lamports = lobby
        .ticket_price
        .checked_mul(count as u64)
        .ok_or(error!(CustomError::Overflow))?;
    system_program::transfer(
        CpiContext::new(
            ctx.accounts.system_program.to_account_info(),
            system_program::Transfer {
                from: ctx.accounts.buyer.to_account_info(),
                to: ctx.accounts.vault.to_account_info(),
            },
        ),
        total_lamports,
    )?;

    // Append `count` cells — all bought by the same wallet, all
    // pointing at the same mint. The Vec capacity was reserved at
    // create time via `Lobby::size(total_cells)` so these pushes
    // never realloc the account.
    let buyer_key = ctx.accounts.buyer.key();
    for _ in 0..count {
        lobby.cells.push(Cell {
            buyer: buyer_key,
            token_mint,
        });
    }
    lobby.sold = lobby
        .sold
        .checked_add(count as u8)
        .ok_or(error!(CustomError::Overflow))?;

    emit!(TicketBought {
        lobby_id: lobby.id,
        buyer: buyer_key,
        token_mint,
        count,
        price_paid: total_lamports,
        sold_after: lobby.sold,
    });

    // Snapshot the fields we'll need for the VRF request so the mut
    // borrow on `lobby` can drop before we hand the whole `ctx.accounts`
    // to `invoke_signed_vrf`.
    let just_filled = lobby.sold == lobby.total_cells;
    if just_filled {
        lobby.status = LobbyStatus::PendingVrf;
    }
    let lobby_id = lobby.id;
    let lobby_sold = lobby.sold;
    let lobby_key = lobby.key();

    if just_filled {
        let callback_accounts = vec![SerializableAccountMeta {
            pubkey: lobby_key,
            is_signer: false,
            is_writable: true,
        }];

        // Anchor-derived discriminator for our `close_lobby` ix. The
        // VRF oracle will land its random value back here.
        let ix = create_request_randomness_ix(RequestRandomnessParams {
            payer: ctx.accounts.buyer.key(),
            oracle_queue: ctx.accounts.oracle_queue.key(),
            callback_program_id: crate::ID,
            callback_discriminator: CloseLobbyDisc::DISCRIMINATOR.to_vec(),
            accounts_metas: Some(callback_accounts),
            caller_seed: {
                // Unique per lobby + per fill — lobby.id (always unique
                // across the program) packed with `sold` as a tail byte
                // for cheap uniqueness padding.
                let mut seed = [0u8; 32];
                seed[..8].copy_from_slice(&lobby_id.to_le_bytes());
                seed[8] = lobby_sold;
                seed
            },
            ..Default::default()
        });
        ctx.accounts
            .invoke_signed_vrf(&ctx.accounts.buyer.to_account_info(), &ix)?;
    }

    Ok(())
}
