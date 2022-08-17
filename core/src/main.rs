// Coding conventions
#![allow(dead_code)]
#![allow(clippy::redundant_clone)]
#![allow(clippy::ptr_arg)]
#![allow(clippy::unit_arg)]
#![forbid(unsafe_code)]
#![deny(non_upper_case_globals)]
#![deny(non_camel_case_types)]
#![deny(non_snake_case)]
#![deny(unused_mut)]
#![deny(unused_imports)]
#![deny(clippy::wildcard_enum_match_arm)]
#![deny(clippy::todo)]
#![deny(clippy::unimplemented)]

#[macro_use]
extern crate lazy_static;

mod actions;
mod address_util;
mod api;
mod box_kind;
mod cli_commands;
mod contracts;
mod datapoint_source;
mod default_parameters;
mod logging;
mod node_interface;
mod oracle_config;
mod oracle_state;
mod pool_commands;
mod scans;
mod serde;
mod state;
mod templates;
#[cfg(test)]
mod tests;
mod wallet;

use actions::execute_action;
use anyhow::anyhow;
use clap::{Parser, Subcommand};
use crossbeam::channel::bounded;
use ergo_lib::ergotree_ir::chain::address::Address;
use ergo_lib::ergotree_ir::chain::address::AddressEncoder;
use ergo_lib::ergotree_ir::chain::token::Token;
use ergo_lib::ergotree_ir::chain::token::TokenId;
use log::debug;
use log::error;
use log::LevelFilter;
use node_interface::assert_wallet_unlocked;
use node_interface::current_block_height;
use node_interface::get_wallet_status;
use node_interface::new_node_interface;
use oracle_state::OraclePool;
use pool_commands::build_action;
use state::process;
use state::PoolState;
use std::convert::TryInto;
use std::thread;
use std::time::Duration;
use wallet::WalletData;

/// A Base58 encoded String of a Ergo P2PK address. Using this type def until sigma-rust matures further with the actual Address type.
pub type P2PKAddress = String;
/// A Base58 encoded String of a Ergo P2S address. Using this type def until sigma-rust matures further with the actual Address type.
pub type P2SAddress = String;
/// The smallest unit of the Erg currency.
pub type NanoErg = u64;
/// A block height of the chain.
pub type BlockHeight = u64;
/// Duration in number of blocks.
pub type BlockDuration = u64;
/// The epoch counter
pub type EpochID = u32;

const APP_VERSION: &str = concat!(
    "v",
    env!("CARGO_PKG_VERSION"),
    "+",
    env!("GIT_COMMIT_HASH"),
    " ",
    env!("GIT_COMMIT_DATE")
);

#[derive(Debug, Parser)]
#[clap(author, version = APP_VERSION, about, long_about = None)]
struct Args {
    #[clap(subcommand)]
    command: Command,
    /// Increase the verbosity of the output to trace log level overriding the log level in the config file.
    #[clap(short, long)]
    verbose: bool,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Bootstrap a new oracle-pool or generate a bootstrap config template file using default
    /// contract scripts and parameters.
    Bootstrap {
        /// The name of the bootstrap config file.
        yaml_config_name: String,
        #[clap(short, long)]
        /// Set this flag to output a bootstrap config template file to the given filename. If
        /// filename already exists, return error.
        generate_config_template: bool,
    },

    /// Run the oracle-pool
    Run {
        #[clap(long)]
        read_only: bool,
    },

    /// Extract reward tokens to a chosen address
    ExtractRewardTokens { rewards_address: String },

    /// Print the number of reward tokens earned by the oracle.
    PrintRewardTokens,

    /// Transfer an oracle token to a chosen address.
    TransferOracleToken { oracle_token_address: String },

    /// Vote to update the oracle pool
    VoteUpdatePool {
        /// The Blake2 hash of the address for the new pool box.
        new_pool_box_address_hash_str: String,
        /// The base-16 representation of the TokenId of the new reward tokens to be used.
        reward_token_id_str: String,
        /// The reward token amount.
        reward_token_amount: u32,
        /// The creation height of the update box.
        update_box_creation_height: u32,
    },
    /// Initiate the Update Pool transaction.
    /// Run with no arguments to show diff between oracle_config.yaml and oracle_config_updated.yaml
    /// Updated config file must be created using --prepare-update command first
    UpdatePool {
        /// New pool box hash. Must match hash of updated pool contract
        new_pool_box_hash: Option<String>,
        /// New reward token id (optional, base64)
        reward_token_id: Option<String>,
        /// New reward token amount, required if new token id was voted for
        reward_token_amount: Option<u64>,
    },
    /// Prepare updating oracle pool with new contracts/parameters.
    PrepareUpdate {
        /// Name of update parameters file (.yaml)
        update_file: String,
    },
}

fn main() {
    let args = Args::parse();
    log::info!("{}", APP_VERSION);

    let cmdline_log_level = if args.verbose {
        Some(LevelFilter::Trace)
    } else {
        None
    };
    logging::setup_log(cmdline_log_level);

    debug!("Args: {:?}", args);

    match args.command {
        Command::Bootstrap {
            yaml_config_name,
            generate_config_template,
        } => {
            if let Err(e) = (|| -> Result<(), anyhow::Error> {
                if generate_config_template {
                    cli_commands::bootstrap::generate_bootstrap_config_template(yaml_config_name)?;
                } else {
                    cli_commands::bootstrap::bootstrap(yaml_config_name)?;
                }
                Ok(())
            })() {
                {
                    error!("Fatal advanced-bootstrap error: {:?}", e);
                    std::process::exit(exitcode::SOFTWARE);
                }
            };
        }

        Command::Run { read_only } => {
            assert_wallet_unlocked(&new_node_interface());
            let (_, repost_receiver) = bounded(1);
            let op = OraclePool::new().unwrap();

            // Start Oracle Core GET API Server
            thread::Builder::new()
                .name("Oracle Core GET API Thread".to_string())
                .spawn(|| {
                    api::start_get_api(repost_receiver);
                })
                .ok();
            loop {
                if let Err(e) = main_loop_iteration(&op, read_only) {
                    error!("Fatal error: {:?}", e);
                    std::process::exit(exitcode::SOFTWARE);
                }
                // Delay loop restart
                thread::sleep(Duration::new(30, 0));
            }
        }

        Command::ExtractRewardTokens { rewards_address } => {
            assert_wallet_unlocked(&new_node_interface());
            let wallet = WalletData {};
            if let Err(e) =
                cli_commands::extract_reward_tokens::extract_reward_tokens(&wallet, rewards_address)
            {
                error!("Fatal extract-rewards-token error: {:?}", e);
                std::process::exit(exitcode::SOFTWARE);
            }
        }

        Command::PrintRewardTokens => {
            assert_wallet_unlocked(&new_node_interface());
            let op = OraclePool::new().unwrap();
            if let Err(e) = cli_commands::print_reward_tokens::print_reward_tokens(
                op.get_local_datapoint_box_source(),
            ) {
                error!("Fatal print-rewards-token error: {:?}", e);
                std::process::exit(exitcode::SOFTWARE);
            }
        }

        Command::TransferOracleToken {
            oracle_token_address,
        } => {
            assert_wallet_unlocked(&new_node_interface());
            let wallet = WalletData {};
            if let Err(e) = cli_commands::transfer_oracle_token::transfer_oracle_token(
                &wallet,
                oracle_token_address,
            ) {
                error!("Fatal transfer-oracle-token error: {:?}", e);
                std::process::exit(exitcode::SOFTWARE);
            }
        }

        Command::VoteUpdatePool {
            new_pool_box_address_hash_str,
            reward_token_id_str,
            reward_token_amount,
            update_box_creation_height,
        } => {
            assert_wallet_unlocked(&new_node_interface());
            let wallet = WalletData {};
            if let Err(e) = cli_commands::vote_update_pool::vote_update_pool(
                &wallet,
                new_pool_box_address_hash_str,
                reward_token_id_str,
                reward_token_amount,
                update_box_creation_height,
            ) {
                error!("Fatal vote-update-pool error: {:?}", e);
                std::process::exit(exitcode::SOFTWARE);
            }
        }
        Command::UpdatePool {
            new_pool_box_hash,
            reward_token_id,
            reward_token_amount,
        } => {
            assert_wallet_unlocked(&new_node_interface());
            let new_reward_tokens =
                reward_token_id
                    .zip(reward_token_amount)
                    .map(|(token_id, amount)| Token {
                        token_id: TokenId::from_base64(&token_id).unwrap(),
                        amount: amount.try_into().unwrap(),
                    });
            if let Err(e) =
                cli_commands::update_pool::update_pool(new_pool_box_hash, new_reward_tokens)
            {
                error!("Fatal update-pool error: {}", e);
                std::process::exit(exitcode::SOFTWARE);
            }
        }
        Command::PrepareUpdate { update_file } => {
            assert_wallet_unlocked(&new_node_interface());
            if let Err(e) = cli_commands::prepare_update::prepare_update(update_file) {
                error!("Fatal update error : {}", e);
                std::process::exit(exitcode::SOFTWARE);
            }
        }
    }
}

fn main_loop_iteration(op: &OraclePool, read_only: bool) -> std::result::Result<(), anyhow::Error> {
    let height = current_block_height()?;
    let wallet = WalletData::new();
    let pool_state = match op.get_live_epoch_state() {
        Ok(live_epoch_state) => PoolState::LiveEpoch(live_epoch_state),
        Err(_) => PoolState::NeedsBootstrap,
    };
    if let Some(cmd) = process(pool_state, height)? {
        let action = build_action(
            cmd,
            op,
            &wallet,
            height as u32,
            get_change_address_from_node()?,
        )?;
        if !read_only {
            execute_action(action)?;
        }
    }
    Ok(())
}

fn get_change_address_from_node() -> Result<Address, anyhow::Error> {
    let change_address_str = get_wallet_status()?
        .change_address
        .ok_or_else(|| anyhow!("failed to get wallet's change address (locked wallet?)"))?;
    let addr = AddressEncoder::unchecked_parse_address_from_str(&change_address_str)?;
    Ok(addr)
}
