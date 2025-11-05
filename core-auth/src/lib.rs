//! # Authentication Module
//!
//! Unified credential manager with pluggable OAuth 2.0 providers.
//!
//! ## Overview
//!
//! This module handles authentication flows for multiple cloud storage providers,
//! including Google Drive and OneDrive. It manages OAuth 2.0 tokens, automatic
//! refresh, and secure credential storage.
//!
//! ## Features
//!
//! - OAuth 2.0 authorization flows with PKCE support
//! - Automatic token refresh before expiration
//! - Secure token storage via platform-specific secure stores
//! - Multi-provider support (Google Drive, OneDrive)
//! - Auth state event emission

pub mod error;
pub mod oauth;
pub mod types;

pub use error::{AuthError, Result};
pub use oauth::{OAuthConfig, OAuthFlowManager, PkceVerifier};
pub use types::{AuthState, OAuthTokens, ProfileId, ProviderKind};
