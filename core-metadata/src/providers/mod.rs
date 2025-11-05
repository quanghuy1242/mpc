//! External Metadata Providers
//!
//! This module contains clients for external metadata services:
//! - MusicBrainz - Music metadata database and Cover Art Archive
//! - Last.fm - Music streaming and metadata service
//!
//! Each provider implements rate limiting and error handling to comply
//! with API terms of service.

#[cfg(feature = "artwork-remote")]
pub mod musicbrainz;

#[cfg(feature = "artwork-remote")]
pub mod lastfm;

#[cfg(feature = "artwork-remote")]
pub use musicbrainz::MusicBrainzClient;

#[cfg(feature = "artwork-remote")]
pub use lastfm::LastFmClient;
