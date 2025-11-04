//! # Core Runtime Module
//!
//! Provides foundational runtime infrastructure for the music platform core:
//! - Logging and tracing infrastructure
//! - Configuration management
//! - Event bus system
//! - Task scheduling primitives
//!
//! ## Overview
//!
//! This crate contains the core runtime utilities that other modules depend on.
//! It establishes the async runtime patterns, logging conventions, and event
//! broadcasting mechanisms used throughout the system.

pub mod config;
pub mod error;
pub mod events;
pub mod logging;

pub use error::{Error, Result};
