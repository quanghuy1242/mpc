//! External Metadata Providers
//!
//! This module contains clients for external metadata services:
//! - MusicBrainz - Music metadata database and Cover Art Archive
//! - Last.fm - Music streaming and metadata service
//! - Artist Enrichment - Artist biography and metadata fetching
//!
//! Each provider implements rate limiting and error handling to comply
//! with API terms of service.

#[cfg(feature = "artwork-remote")]
pub mod musicbrainz;

#[cfg(feature = "artwork-remote")]
pub mod lastfm;

pub mod artist_enrichment;

#[cfg(feature = "artwork-remote")]
pub use musicbrainz::MusicBrainzClient;

#[cfg(feature = "artwork-remote")]
pub use lastfm::LastFmClient;

pub use artist_enrichment::{ArtistEnrichmentProvider, ArtistMetadata};
