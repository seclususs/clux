//! This file is part of QoS-Android.
//! Licensed under the GNU GPL v3.

#![warn(clippy::pedantic)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::similar_names)]
#![allow(clippy::module_name_repetitions)]

pub mod algorithms;
pub mod bindings;
pub mod config;
pub mod controllers;
pub mod daemon;
pub mod hal;

pub use bindings::ffi::*;
