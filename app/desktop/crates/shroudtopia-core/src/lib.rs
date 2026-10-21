//! Shroudtopia desktop core: everything the app does that is not Win32 or Tauri, so it is tested on Linux.
//!
//! - `steam` / `vdf`: the Steam account, its saves and the game's folder, read from Steam's files;
//! - `saves`: the two stores a shared world can live in (Steam Cloud slot, local saves folder);
//! - `contract` / `api`: the sync server's shapes and routes (`app/CONTRACT.md`);
//! - `engine` / `sync_state` / `view`: the sync loop, its seven states, and what the window shows;
//! - `mods`: the loader's `shroudtopia.json`;
//! - `signature` / `updater`: signed releases from GitHub, the mods' install and the app's own swap.

pub mod api;
pub mod config;
pub mod contract;
pub mod engine;
pub mod format;
pub mod mods;
pub mod saves;
pub mod signature;
pub mod steam;
pub mod sync_state;
pub mod testing;
pub mod updater;
pub mod vdf;
pub mod view;

/// The app's version (`App v0.1.0`), the one releases are compared with.
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
