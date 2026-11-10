//! # zeroin-core
//!
//! Platform-independent logic for ZeroIn: the game database, sensitivity
//! math, aim-drill simulation, scoring, the deterministic sensitivity search,
//! validation of local-model advice, polling-rate statistics, spring
//! animation physics, and the history file format.
//!
//! Nothing in this crate touches the operating system beyond reading and
//! writing the files it is explicitly given, so every piece can be unit
//! tested on any platform.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod advisor;
pub mod cursor;
pub mod drills;
pub mod games;
pub mod history;
pub mod math;
pub mod polling;
pub mod rng;
pub mod scoring;
pub mod search;
pub mod spring;
pub mod winmouse;
