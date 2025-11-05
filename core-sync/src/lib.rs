//! # Sync & Indexing Module
//!
//! Orchestrates synchronization with cloud storage providers.
//!
//! ## Overview
//!
//! This module manages the lifecycle of sync jobs, including:
//! - Listing remote files via `StorageProvider`
//! - Filtering audio files by MIME type and extension
//! - Extracting metadata from downloaded files
//! - Resolving conflicts (renames, duplicates, deletions)
//! - Persisting library entries to the database
//!
//! ## Components
//!
//! - **Sync Job State Machine** (`job`): Manages sync job lifecycle with validated state transitions
//! - **Scan Queue** (`scan_queue`): Work queue for processing discovered files with retry logic
//! - **Conflict Resolver** (`conflict_resolver`): Handles renames, duplicates, and deletions
//! - **Repository** (`repository`): Database persistence for sync jobs and queue items
//! - **Sync Coordinator** (`coordinator`): Orchestrates full and incremental synchronization

pub mod conflict_resolution_orchestrator;
pub mod conflict_resolver;
pub mod coordinator;
pub mod error;
pub mod job;
pub mod metadata_processor;
pub mod repository;
pub mod scan_queue;

pub use error::{Result, SyncError};
pub use job::{
    SyncJob, SyncJobId, SyncJobStats, SyncProgress, SyncStatus, SyncType,
};
pub use repository::{SyncJobRepository, SqliteSyncJobRepository};
pub use scan_queue::{
    Priority, QueueStats, ScanQueue, ScanQueueRepository, SqliteScanQueueRepository,
    WorkItem, WorkItemId, WorkItemStatus,
};
pub use conflict_resolution_orchestrator::{
    ConflictResolutionOrchestrator, ConflictResolutionStats,
};
pub use conflict_resolver::{
    ConflictPolicy, ConflictResolver, DuplicateSet, MetadataConflict, ResolutionResult,
};
pub use coordinator::{SyncConfig, SyncCoordinator};
pub use metadata_processor::{MetadataProcessor, ProcessingResult, ProcessorConfig};
