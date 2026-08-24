//! `GameLua` text-resource and JSON import bridge.
//!
//! Purple entry points:
//! - `sub_1000512D8`: raw/encrypted text byte pipeline
//! - `sub_100051810`: text file to Lua value
//! - `sub_100057450`: JSON document into an existing named table
//! - `sub_10052AE8C`: compile/execute a returned Lua chunk

mod imports;
mod paths;
mod pipeline;

pub(crate) use imports::{install_data_imports, install_string_loader};
