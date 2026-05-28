//! VRF callback — the ephemeral_vrf_sdk oracle invokes this ix with a
//! 32-byte random buffer once the request fired in `buy_ticket` has
//! resolved. We pick the winning cell index, flip `status → Closed`
//! and stamp `winner_idx`. The actual SOL → token swap + payout
//! happens later in `settle_lobby` (keeper-driven, Jupiter CPI).
//!
//! Only the VRF oracle program can call this — enforced by pinning
//! the signer address to `VRF_PROGRAM_IDENTITY`. Any other caller
//! gets rejected at account validation.

use anchor_lang::prelude::*;
use ephemeral_vrf_sdk::consts::VRF_PROGRAM_IDENTITY;

use crate::constants::*;
use crate::errors::*;
use crate::events::LobbyClosed;
use crate::state::*;

#[derive(Accounts)]
pub struct CloseLobby<'info>
{
    /// VRF program signature — pinning the address means the only key
    /// allowed to sign this ix is the oracle program identity. Nobody
    /// else can craft a tx that resolves a lobby.
    #[account(address = VRF_PROGRAM_IDENTITY)]
    pub vrf_program_identity: Signer<'info>,

    #[account(
        mut,
        seeds = [LOBBY_SEED, lobby.id.to_le_bytes().as_ref()],
        bump = lobby.bump,
    )]
    pub lobby: Account<'info, Lobby>,
}

pub fn handler(ctx: Context<CloseLobby>, random_value: [u8; 32]) -> Result<()>
{
    let lobby = &mut ctx.accounts.lobby;
    require!(
        lobby.status == LobbyStatus::PendingVrf,
        CustomError::LobbyNotPendingVrf,
    );
    // Defense-in-depth: PendingVrf is only set when the lobby fills, so
    // by construction `cells.len() == sold == total_cells`. We re-check
    // before indexing `cells[winner_idx]` so a corrupt state can't make
    // the VRF callback panic.
    require!(
        lobby.cells.len() == lobby.sold as usize,
        CustomError::CellsLenMismatch,
    );

    // Fold the first 8 bytes of randomness into a u64, modulo by
    // total_cells. Bias from `% total_cells` on a uniform u64
    let random_u64 = u64::from_le_bytes(random_value[..8].try_into().unwrap());
    lobby.winner_idx = (random_u64 % lobby.total_cells as u64) as u8;
    lobby.status = LobbyStatus::Closed;

    // Surface the result so the front can react without polling. The
    // winning cell's `buyer` + `token_mint` are inlined into the event
    // so the front doesn't need to re-fetch the full Lobby account.
    let winning_cell = lobby.cells[lobby.winner_idx as usize];
    emit!(LobbyClosed {
        lobby_id: lobby.id,
        winner_idx: lobby.winner_idx,
        winner: winning_cell.buyer,
        token_mint: winning_cell.token_mint,
    });

    Ok(())
}
