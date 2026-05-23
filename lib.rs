//! twentyfivex — 25x.fun on-chain program.
//!
//! Game model: an admin spins up a lobby with a custom `ticket_price` and
//! `total_cells`. Buyers fill cells one ticket at a time, each picking the
//! token mint their cell points at. When the lobby fills, the program
//! draws a winning cell (slothashes-based randomness for the MVP — swap
//! for Switchboard later) and a keeper settles by swapping the SOL pot
//! into the winner's chosen token and transferring it out.
//!
//! Module layout: `state` holds the account shapes, one file per ix in
//! `instructions/` (`Accounts` struct + handler), `constants` and
//! `errors` are shared helpers.

use anchor_lang::prelude::*;

pub mod constants;
pub mod errors;
pub mod events;
pub mod instructions;
pub mod state;

use instructions::*;

declare_id!("2RwXpKJkPTHsoJ4rCdFVDZRL5XC78qEfRy8MxmpvLa6v");

#[program]
pub mod twentyfivex {
    use super::*;

    pub fn init_config(ctx: Context<InitConfig>) -> Result<()> {
        instructions::init_config::handler(ctx)
    }

    pub fn create_lobby(
        ctx: Context<CreateLobby>,
        ticket_price: u64,
        total_cells: u8,
    ) -> Result<()> {
        instructions::create_lobby::handler(ctx, ticket_price, total_cells)
    }

    pub fn buy_ticket(
        ctx: Context<BuyTicket>,
        token_mint: Pubkey,
        count: u16,
    ) -> Result<()> {
        instructions::buy_ticket::handler(ctx, token_mint, count)
    }

    pub fn buy_ticket_with_sponsor(
        ctx: Context<BuyTicketWithSponsor>,
        token_mint: Pubkey,
        count: u16,
    ) -> Result<()> {
        instructions::buy_ticket_with_sponsor::handler(ctx, token_mint, count)
    }

    pub fn sell_ticket(ctx: Context<SellTicket>, count: u16) -> Result<()> {
        instructions::sell_ticket::handler(ctx, count)
    }

    pub fn close_lobby(ctx: Context<CloseLobby>, random_value: [u8; 32]) -> Result<()> {
        instructions::close_lobby::handler(ctx, random_value)
    }

    pub fn settle_lobby(ctx: Context<SettleLobby>) -> Result<()> {
        instructions::settle_lobby::handler(ctx)
    }

    pub fn cancel_lobby<'info>(
        ctx: Context<'_, '_, '_, 'info, CancelLobby<'info>>,
    ) -> Result<()> {
        instructions::cancel_lobby::handler(ctx)
    }

    pub fn update_admin(ctx: Context<UpdateAdmin>) -> Result<()> {
        instructions::update_admin::handler(ctx)
    }

    pub fn update_keeper(ctx: Context<UpdateKeeper>) -> Result<()> {
        instructions::update_keeper::handler(ctx)
    }
}
