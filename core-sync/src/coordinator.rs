//! # Sync Coordinator
//!
//! Orchestrates full and incremental synchronization with cloud storage providers.
//!
//! ## Overview
//!
//! The `SyncCoordinator` is the central orchestrator for sync operations. It coordinates
//! between multiple modules to:
//! - Authenticate with cloud providers via `AuthManager`
//! - List files using `StorageProvider` implementations
//! - Filter audio files by MIME type and extension
//! - Enqueue files to `ScanQueue` for processing
//! - Extract metadata and persist to library database
//! - Handle conflicts via `ConflictResolver`
//! - Update sync cursors for incremental sync
//! - Emit progress events via `EventBus`
//!
//! ## Workflow
//!
//! ### Full Sync
//! 1. Acquire valid access token from `AuthManager`
//! 2. List all files from provider (paginated)
//! 3. Filter for audio files (MIME type, extensions)
//! 4. Enqueue work items to `ScanQueue`
//! 5. Process queue concurrently with throttling
//! 6. Download and extract metadata for each file
//! 7. Resolve conflicts (duplicates, renames)
//! 8. Persist tracks to library database
//! 9. Update provider sync cursor
//! 10. Emit completion event
//!
//! ### Incremental Sync
//! 1. Retrieve last sync cursor from provider record
//! 2. Get changes since cursor from provider
//! 3. Process added/modified/deleted files
//! 4. Update existing records or add new ones
//! 5. Update cursor for next incremental sync
//!
//! ## Usage
//!
//! ```rust,ignore
//! use core_sync::SyncCoordinator;
//! use core_auth::ProfileId;
//! use std::sync::Arc;
//!
//! # async fn example(coordinator: Arc<SyncCoordinator>) -> Result<(), Box<dyn std::error::Error>> {
//! // Start full sync
//! let profile_id = ProfileId::new();
//! let job_id = coordinator.start_full_sync(profile_id).await?;
//!
//! // Check status
//! let status = coordinator.get_status(job_id).await?;
//! println!("Progress: {}%", status.progress.percent);
//!
//! // Cancel if needed
//! coordinator.cancel_sync(job_id).await?;
//! # Ok(())
//! # }
//! ```

use crate::{
    conflict_resolver::{ConflictPolicy, ConflictResolver},
    job::{SyncJob, SyncJobId, SyncJobStats, SyncType},
    repository::{SqliteSyncJobRepository, SyncJobRepository},
    scan_queue::{ScanQueue, WorkItem},
    Result, SyncError,
};
use bridge_traits::{
    network::{NetworkMonitor, NetworkStatus, NetworkType},
    storage::{RemoteFile, StorageProvider},
};
use core_auth::{AuthManager, ProfileId, ProviderKind};
use core_runtime::events::{CoreEvent, EventBus, SyncEvent};
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, RwLock};
use tokio::time::timeout;
use tracing::{debug, error, info, instrument, warn};

/// Sync coordinator configuration
#[derive(Debug, Clone)]
pub struct SyncConfig {
    /// Maximum concurrent file processing operations
    pub max_concurrent_downloads: usize,

    /// Timeout for entire sync operation (seconds)
    pub sync_timeout_secs: u64,

    /// Timeout for individual file download (seconds)
    pub download_timeout_secs: u64,

    /// Whether to sync only on unmetered networks (WiFi)
    pub wifi_only: bool,

    /// Maximum file size to process (bytes). Files larger than this are skipped.
    pub max_file_size_bytes: u64,

    /// Audio file MIME types to include
    pub audio_mime_types: Vec<String>,

    /// Audio file extensions to include
    pub audio_extensions: Vec<String>,
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            max_concurrent_downloads: 4,
            sync_timeout_secs: 3600, // 1 hour
            download_timeout_secs: 60,
            wifi_only: false,
            max_file_size_bytes: 500 * 1024 * 1024, // 500 MB
            audio_mime_types: vec![
                "audio/mpeg".to_string(),
                "audio/mp3".to_string(),
                "audio/flac".to_string(),
                "audio/x-flac".to_string(),
                "audio/ogg".to_string(),
                "audio/x-vorbis+ogg".to_string(),
                "audio/vorbis".to_string(),
                "audio/mp4".to_string(),
                "audio/m4a".to_string(),
                "audio/x-m4a".to_string(),
                "audio/aac".to_string(),
                "audio/wav".to_string(),
                "audio/x-wav".to_string(),
                "audio/wave".to_string(),
                "audio/webm".to_string(),
                "audio/opus".to_string(),
            ],
            audio_extensions: vec![
                "mp3".to_string(),
                "flac".to_string(),
                "ogg".to_string(),
                "oga".to_string(),
                "opus".to_string(),
                "m4a".to_string(),
                "aac".to_string(),
                "wav".to_string(),
                "wave".to_string(),
                "wma".to_string(),
                "alac".to_string(),
                "aiff".to_string(),
                "aif".to_string(),
                "ape".to_string(),
                "wv".to_string(),
            ],
        }
    }
}

/// Active sync job tracking
#[derive(Clone)]
struct ActiveSync {
    job_id: SyncJobId,
    profile_id: ProfileId,
    cancellation_token: tokio_util::sync::CancellationToken,
}

/// Sync coordinator for orchestrating synchronization
pub struct SyncCoordinator {
    /// Configuration
    config: SyncConfig,

    /// Authentication manager
    auth_manager: Arc<AuthManager>,

    /// Event bus for emitting sync events
    event_bus: Arc<EventBus>,

    /// Network monitor for connectivity checks
    network_monitor: Option<Arc<dyn NetworkMonitor>>,

    /// Storage providers by kind
    providers: Arc<RwLock<HashMap<ProviderKind, Arc<dyn StorageProvider>>>>,

    /// Database connection pool
    db_pool: SqlitePool,

    /// Active sync jobs by profile
    active_syncs: Arc<Mutex<HashMap<ProfileId, ActiveSync>>>,

    /// Scan queue for work items
    scan_queue: Arc<ScanQueue>,

    /// Conflict resolver
    conflict_resolver: Arc<ConflictResolver>,

    /// Sync job repository
    job_repository: Arc<SqliteSyncJobRepository>,
}

impl SyncCoordinator {
    /// Create a new sync coordinator
    ///
    /// # Arguments
    ///
    /// * `config` - Sync configuration
    /// * `auth_manager` - Authentication manager for token acquisition
    /// * `event_bus` - Event bus for emitting sync progress events
    /// * `network_monitor` - Optional network monitor for connectivity checks
    /// * `db_pool` - Database connection pool
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use core_sync::{SyncCoordinator, SyncConfig};
    /// use std::sync::Arc;
    ///
    /// let coordinator = SyncCoordinator::new(
    ///     SyncConfig::default(),
    ///     auth_manager,
    ///     event_bus,
    ///     Some(network_monitor),
    ///     db_pool,
    /// ).await?;
    /// ```
    pub async fn new(
        config: SyncConfig,
        auth_manager: Arc<AuthManager>,
        event_bus: Arc<EventBus>,
        network_monitor: Option<Arc<dyn NetworkMonitor>>,
        db_pool: SqlitePool,
    ) -> Result<Self> {
        let scan_queue = Arc::new(
            ScanQueue::new(db_pool.clone(), config.max_concurrent_downloads).await?,
        );

        let conflict_resolver = Arc::new(ConflictResolver::new(
            db_pool.clone(),
            ConflictPolicy::KeepNewest,
        ));

        let job_repository = Arc::new(SqliteSyncJobRepository::new(db_pool.clone()));

        Ok(Self {
            config,
            auth_manager,
            event_bus,
            network_monitor,
            providers: Arc::new(RwLock::new(HashMap::new())),
            db_pool,
            active_syncs: Arc::new(Mutex::new(HashMap::new())),
            scan_queue,
            conflict_resolver,
            job_repository,
        })
    }

    /// Register a storage provider
    ///
    /// Storage providers must be registered before starting sync operations.
    ///
    /// # Arguments
    ///
    /// * `kind` - Provider type (GoogleDrive, OneDrive)
    /// * `provider` - Storage provider implementation
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use core_auth::ProviderKind;
    /// use provider_google_drive::GoogleDriveConnector;
    ///
    /// let provider = Arc::new(GoogleDriveConnector::new(http_client));
    /// coordinator.register_provider(ProviderKind::GoogleDrive, provider).await;
    /// ```
    pub async fn register_provider(
        &self,
        kind: ProviderKind,
        provider: Arc<dyn StorageProvider>,
    ) {
        let mut providers = self.providers.write().await;
        providers.insert(kind, provider);
        info!("Registered storage provider: {}", kind);
    }

    /// Start a full synchronization for a profile
    ///
    /// Performs initial scan of all files from the cloud provider.
    /// Creates a new sync job and returns its ID immediately.
    /// The actual sync runs in the background.
    ///
    /// # Arguments
    ///
    /// * `profile_id` - User profile to sync
    ///
    /// # Returns
    ///
    /// Returns the `SyncJobId` for tracking progress
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Another sync is already in progress for this profile
    /// - Profile is not authenticated
    /// - Provider is not registered
    /// - Network constraints are not met (if wifi_only is enabled)
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let job_id = coordinator.start_full_sync(profile_id).await?;
    /// println!("Started sync job: {}", job_id);
    /// ```
    #[instrument(skip(self), fields(profile_id = %profile_id))]
    pub async fn start_full_sync(&self, profile_id: ProfileId) -> Result<SyncJobId> {
        self.start_sync_internal(profile_id, SyncType::Full, None)
            .await
    }

    /// Start an incremental synchronization for a profile
    ///
    /// Processes only changes since the last sync cursor.
    /// More efficient than full sync for detecting updates.
    ///
    /// # Arguments
    ///
    /// * `profile_id` - User profile to sync
    /// * `cursor` - Optional sync cursor from previous sync. If None, uses stored cursor.
    ///
    /// # Returns
    ///
    /// Returns the `SyncJobId` for tracking progress
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Another sync is already in progress for this profile
    /// - Profile is not authenticated
    /// - Provider is not registered
    /// - No previous sync cursor available (must do full sync first)
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let job_id = coordinator.start_incremental_sync(profile_id, None).await?;
    /// println!("Started incremental sync: {}", job_id);
    /// ```
    #[instrument(skip(self), fields(profile_id = %profile_id))]
    pub async fn start_incremental_sync(
        &self,
        profile_id: ProfileId,
        cursor: Option<String>,
    ) -> Result<SyncJobId> {
        self.start_sync_internal(profile_id, SyncType::Incremental, cursor)
            .await
    }

    /// Internal method to start sync operation
    async fn start_sync_internal(
        &self,
        profile_id: ProfileId,
        sync_type: SyncType,
        cursor: Option<String>,
    ) -> Result<SyncJobId> {
        // Check if sync already in progress
        {
            let active_syncs = self.active_syncs.lock().await;
            if active_syncs.contains_key(&profile_id) {
                return Err(SyncError::SyncInProgress {
                    profile_id: profile_id.to_string(),
                });
            }
        }

        // Check network constraints
        if self.config.wifi_only {
            if let Some(monitor) = &self.network_monitor {
                let network_info = monitor.get_network_info().await
                    .map_err(|e| SyncError::Provider(format!("Failed to check network: {}", e)))?;
                
                if network_info.status != NetworkStatus::Connected {
                    return Err(SyncError::Provider(
                        "Network not available".to_string(),
                    ));
                }
                
                // WiFi-only mode: require WiFi connection and non-metered
                if !matches!(network_info.network_type, Some(NetworkType::WiFi)) {
                    return Err(SyncError::Provider(
                        "WiFi-only mode enabled but not connected to WiFi".to_string(),
                    ));
                }
                
                if network_info.is_metered {
                    return Err(SyncError::Provider(
                        "WiFi-only mode enabled but network is metered".to_string(),
                    ));
                }
            }
        }

        // Get current session and provider
        let session = self
            .auth_manager
            .current_session()
            .await
            .ok_or_else(|| SyncError::Provider("No active session".to_string()))?;

        // Verify provider is registered
        {
            let providers = self.providers.read().await;
            if !providers.contains_key(&session.provider) {
                return Err(SyncError::Provider(format!(
                    "Provider {} not registered",
                    session.provider
                )));
            }
        }

        // Create sync job
        let mut job = match sync_type {
            SyncType::Full => SyncJob::new(session.provider, SyncType::Full),
            SyncType::Incremental => SyncJob::new_incremental(session.provider, cursor.ok_or_else(|| SyncError::InvalidInput {
                field: "cursor".to_string(),
                message: "Cursor required for incremental sync".to_string(),
            })?),
        };

        // Start job
        job = job.start()?;
        let job_id = job.id;

        // Persist job
        self.job_repository.insert(&job).await?;

        // Create cancellation token
        let cancellation_token = tokio_util::sync::CancellationToken::new();

        // Track active sync
        {
            let mut active_syncs = self.active_syncs.lock().await;
            active_syncs.insert(
                profile_id,
                ActiveSync {
                    job_id,
                    profile_id,
                    cancellation_token: cancellation_token.clone(),
                },
            );
        }

        // Emit started event
        self.event_bus
            .emit(CoreEvent::Sync(SyncEvent::Started {
                job_id: job_id.to_string(),
                profile_id: profile_id.to_string(),
                provider: session.provider.to_string(),
                is_full_sync: matches!(sync_type, SyncType::Full),
            }))
            .ok();

        // Spawn background task
        let coordinator = Arc::new(self.clone_for_task());
        tokio::spawn(async move {
            let result = coordinator
                .run_sync_task(job_id, profile_id, cancellation_token)
                .await;

            // Clean up active sync tracking
            {
                let mut active_syncs = coordinator.active_syncs.lock().await;
                active_syncs.remove(&profile_id);
            }

            if let Err(e) = result {
                error!("Sync task failed: {}", e);
            }
        });

        info!(
            "Started {} sync for profile {} with job {}",
            sync_type, profile_id, job_id
        );

        Ok(job_id)
    }

    /// Clone for background task (avoids Arc<Arc<...>>)
    fn clone_for_task(&self) -> Self {
        Self {
            config: self.config.clone(),
            auth_manager: Arc::clone(&self.auth_manager),
            event_bus: Arc::clone(&self.event_bus),
            network_monitor: self.network_monitor.clone(),
            providers: Arc::clone(&self.providers),
            db_pool: self.db_pool.clone(),
            active_syncs: Arc::clone(&self.active_syncs),
            scan_queue: Arc::clone(&self.scan_queue),
            conflict_resolver: Arc::clone(&self.conflict_resolver),
            job_repository: Arc::clone(&self.job_repository),
        }
    }

    /// Run sync task in background
    #[instrument(skip(self, cancellation_token), fields(job_id = %job_id, profile_id = %profile_id))]
    async fn run_sync_task(
        &self,
        job_id: SyncJobId,
        profile_id: ProfileId,
        cancellation_token: tokio_util::sync::CancellationToken,
    ) -> Result<()> {
        // Wrap in timeout
        let sync_future = self.execute_sync(job_id, profile_id, cancellation_token);

        match timeout(
            Duration::from_secs(self.config.sync_timeout_secs),
            sync_future,
        )
        .await
        {
            Ok(Ok(())) => {
                info!("Sync job {} completed successfully", job_id);
                Ok(())
            }
            Ok(Err(e)) => {
                error!("Sync job {} failed: {}", job_id, e);

                // Update job status
                if let Ok(Some(job)) = self.job_repository.find_by_id(&job_id).await {
                    if let Ok(failed_job) = job.fail(e.to_string(), None) {
                        let _ = self.job_repository.update(&failed_job).await;

                        // Emit failed event
                        self.event_bus
                            .emit(CoreEvent::Sync(SyncEvent::Failed {
                                job_id: job_id.to_string(),
                                message: e.to_string(),
                                items_processed: failed_job.progress.items_processed,
                                recoverable: true,
                            }))
                            .ok();
                    }
                }

                Err(e)
            }
            Err(_) => {
                error!("Sync job {} timed out", job_id);

                // Update job status
                if let Ok(Some(job)) = self.job_repository.find_by_id(&job_id).await {
                    let timeout_msg = format!("Timeout after {} seconds", self.config.sync_timeout_secs);
                    if let Ok(failed_job) = job.fail(timeout_msg.clone(), None) {
                        let _ = self.job_repository.update(&failed_job).await;
                    }
                }

                Err(SyncError::Timeout(self.config.sync_timeout_secs))
            }
        }
    }

    /// Execute the sync operation
    #[instrument(skip(self, cancellation_token))]
    async fn execute_sync(
        &self,
        job_id: SyncJobId,
        profile_id: ProfileId,
        cancellation_token: tokio_util::sync::CancellationToken,
    ) -> Result<()> {
        // Get current session
        let session = self
            .auth_manager
            .current_session()
            .await
            .ok_or_else(|| SyncError::Provider("No active session".to_string()))?;

        // Get provider
        let provider = {
            let providers = self.providers.read().await;
            providers
                .get(&session.provider)
                .ok_or_else(|| {
                    SyncError::Provider(format!("Provider {} not available", session.provider))
                })?
                .clone()
        };

        // Get current job
        let mut job = self
            .job_repository
            .find_by_id(&job_id)
            .await?
            .ok_or_else(|| SyncError::JobNotFound {
                job_id: job_id.to_string(),
            })?;

        // Phase 1: List files
        info!("Phase 1: Listing files from provider");
        let mut all_files = Vec::new();
        let mut cursor = job.cursor.clone();
        let mut page_count = 0;

        loop {
            if cancellation_token.is_cancelled() {
                return Err(SyncError::Cancelled);
            }

            page_count += 1;
            debug!("Fetching page {} (cursor: {:?})", page_count, cursor);

            let (files, next_cursor) = provider.list_media(cursor.clone()).await.map_err(|e| {
                SyncError::Provider(format!("Failed to list media: {}", e))
            })?;

            all_files.extend(files);

            // Update progress
            job.update_progress(
                all_files.len() as u64,
                0, // Total unknown during discovery
                &format!("Discovered {} files", all_files.len()),
            )?;
            self.job_repository.update(&job).await?;

            // Emit progress event
            self.event_bus
                .emit(CoreEvent::Sync(SyncEvent::Progress {
                    job_id: job_id.to_string(),
                    items_processed: all_files.len() as u64,
                    total_items: None, // Unknown during discovery
                    percent: 0,
                    phase: "discovering".to_string(),
                }))
                .ok();

            cursor = next_cursor;
            if cursor.is_none() {
                break;
            }
        }

        info!("Discovered {} total files", all_files.len());

        // Phase 2: Filter audio files
        info!("Phase 2: Filtering audio files");
        let audio_files = self.filter_audio_files(all_files);
        info!("Filtered to {} audio files", audio_files.len());

        if audio_files.is_empty() {
            warn!("No audio files found, completing sync");
            let stats = SyncJobStats {
                items_added: 0,
                items_updated: 0,
                items_deleted: 0,
                items_failed: 0,
            };
            let completed_job = job.complete(stats.clone())?;
            self.job_repository.update(&completed_job).await?;

            self.event_bus
                .emit(CoreEvent::Sync(SyncEvent::Completed {
                    job_id: job_id.to_string(),
                    items_processed: 0,
                    items_added: 0,
                    items_updated: 0,
                    items_deleted: 0,
                    duration_secs: completed_job.duration_secs().unwrap_or(0),
                }))
                .ok();

            return Ok(());
        }

        // Phase 3: Enqueue work items
        info!("Phase 3: Enqueueing {} work items", audio_files.len());
        for file in audio_files.iter() {
            if cancellation_token.is_cancelled() {
                return Err(SyncError::Cancelled);
            }

            let work_item = WorkItem::new(
                file.id.clone(),
                file.mime_type.clone().unwrap_or_else(|| "application/octet-stream".to_string()),
            )
            .with_file_size(file.size.unwrap_or(0) as i64);

            self.scan_queue.enqueue(work_item).await?;
        }

        // Phase 4: Process queue (stub - actual implementation would extract metadata)
        info!("Phase 4: Processing queue");
        let total_items = audio_files.len() as u64;
        let mut processed = 0u64;
        let mut added = 0u64;
        let updated = 0u64;
        let mut failed = 0u64;

        // For now, just mark items as complete
        // TODO(TASK-401): Implement actual metadata extraction and persistence
        // See: docs/ai_task_list.md TASK-401 for implementation plan
        loop {
            if cancellation_token.is_cancelled() {
                return Err(SyncError::Cancelled);
            }

            match self.scan_queue.dequeue().await {
                Ok(Some(item)) => {
                    processed += 1;
                    debug!("Processing work item: {} ({}/{})", item.remote_file_id, processed, total_items);

                    // TODO(TASK-401): Download file, extract metadata, persist to database
                    // Future implementation:
                    // 1. Download: provider.download(&item.remote_file_id).await?
                    // 2. Extract: MetadataExtractor::extract_from_file(&temp_path).await?
                    // 3. Persist: track_repository.insert(track).await?
                    // 4. Hash: calculate content hash for deduplication
                    // For now, just mark as complete
                    // For now, just mark as complete
                    match self.scan_queue.mark_complete(item.id).await {
                        Ok(_) => {
                            added += 1;
                        }
                        Err(e) => {
                            warn!("Failed to mark item complete: {}", e);
                            failed += 1;
                            let _ = self
                                .scan_queue
                                .mark_failed(item.id, Some(e.to_string()))
                                .await;
                        }
                    }

                    // Update progress
                    let percent = ((processed as f64 / total_items as f64) * 100.0) as u8;
                    job.update_progress(
                        processed,
                        total_items,
                        &format!("Processed {}/{} files", processed, total_items),
                    )?;
                    self.job_repository.update(&job).await?;

                    // Emit progress event
                    self.event_bus
                        .emit(CoreEvent::Sync(SyncEvent::Progress {
                            job_id: job_id.to_string(),
                            items_processed: processed,
                            total_items: Some(total_items),
                            percent,
                            phase: "processing".to_string(),
                        }))
                        .ok();
                }
                Ok(None) => {
                    // No more items in queue
                    break;
                }
                Err(e) => {
                    error!("Error dequeuing item: {}", e);
                    break;
                }
            }
        }

        // Phase 5: Detect and resolve conflicts
        info!("Phase 5: Resolving conflicts");
        // TODO(Enhancement): Integrate ConflictResolver workflow
        // ConflictResolver is fully implemented (TASK-303), needs integration:
        // 1. Detect duplicates: conflict_resolver.detect_duplicates(&db_pool).await?
        // 2. Resolve renames: conflict_resolver.resolve_rename(track_id, new_file_id, ...).await?
        // 3. Handle deletions: conflict_resolver.handle_deletion(track_id, soft=true, ...).await?
        // See: memory_task_304_sync_coordinator.md for detailed implementation plan

        // Phase 6: Complete sync job
        info!("Phase 6: Completing sync job");
        let stats = SyncJobStats {
            items_added: added,
            items_updated: updated,
            items_deleted: 0, // TODO(Enhancement): Track deletions during conflict resolution
            items_failed: failed,
        };

        let completed_job = job.complete(stats.clone())?;
        self.job_repository.update(&completed_job).await?;

        let duration_secs = chrono::Utc::now().timestamp() - completed_job.started_at.unwrap_or(0);

        // Emit completion event
        self.event_bus
            .emit(CoreEvent::Sync(SyncEvent::Completed {
                job_id: job_id.to_string(),
                items_processed: processed,
                items_added: added,
                items_updated: updated,
                items_deleted: 0, // TODO(Enhancement): Track from conflict resolution phase
                duration_secs: duration_secs.max(0) as u64,
            }))
            .ok();

        info!(
            "Sync job {} completed: {} added, {} updated, {} failed",
            job_id, stats.items_added, stats.items_updated, stats.items_failed
        );

        Ok(())
    }

    /// Filter files to only audio types
    fn filter_audio_files(&self, files: Vec<RemoteFile>) -> Vec<RemoteFile> {
        files
            .into_iter()
            .filter(|file| {
                // Skip folders
                if file.is_folder {
                    return false;
                }

                // Skip files exceeding size limit
                if let Some(size) = file.size {
                    if size > self.config.max_file_size_bytes {
                        debug!(
                            "Skipping file {} (size {} exceeds limit {})",
                            file.name, size, self.config.max_file_size_bytes
                        );
                        return false;
                    }
                }

                // Check MIME type
                if let Some(ref mime) = file.mime_type {
                    if self.config.audio_mime_types.iter().any(|m| mime == m) {
                        return true;
                    }
                }

                // Check extension
                if let Some(ext) = file.name.rsplit('.').next() {
                    let ext_lower = ext.to_lowercase();
                    if self.config.audio_extensions.iter().any(|e| e == &ext_lower) {
                        return true;
                    }
                }

                false
            })
            .collect()
    }

    /// Cancel a running sync job
    ///
    /// Gracefully cancels a sync operation, allowing current work items to complete
    /// but preventing new items from being processed.
    ///
    /// # Arguments
    ///
    /// * `job_id` - Sync job to cancel
    ///
    /// # Errors
    ///
    /// Returns an error if the job is not found or not running
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// coordinator.cancel_sync(job_id).await?;
    /// println!("Sync cancelled");
    /// ```
    #[instrument(skip(self), fields(job_id = %job_id))]
    pub async fn cancel_sync(&self, job_id: SyncJobId) -> Result<()> {
        // Find active sync
        let active_sync = {
            let active_syncs = self.active_syncs.lock().await;
            active_syncs
                .values()
                .find(|sync| sync.job_id == job_id)
                .cloned()
        };

        if let Some(sync) = active_sync {
            // Cancel the task
            sync.cancellation_token.cancel();

            // Update job status
            if let Ok(Some(job)) = self.job_repository.find_by_id(&job_id).await {
                if let Ok(cancelled_job) = job.cancel() {
                    self.job_repository.update(&cancelled_job).await?;
                }
            }

            // Emit cancelled event
            self.event_bus
                .emit(CoreEvent::Sync(SyncEvent::Cancelled {
                    job_id: job_id.to_string(),
                    items_processed: 0, // We don't track partial progress on cancel
                }))
                .ok();

            info!("Cancelled sync job {}", job_id);
            Ok(())
        } else {
            Err(SyncError::JobNotFound {
                job_id: job_id.to_string(),
            })
        }
    }

    /// Get the current status of a sync job
    ///
    /// # Arguments
    ///
    /// * `job_id` - Sync job to query
    ///
    /// # Returns
    ///
    /// Returns the current `SyncJob` with progress information
    ///
    /// # Errors
    ///
    /// Returns an error if the job is not found
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let job = coordinator.get_status(job_id).await?;
    /// println!("Progress: {}%", job.progress.percent);
    /// ```
    pub async fn get_status(&self, job_id: SyncJobId) -> Result<SyncJob> {
        self.job_repository
            .find_by_id(&job_id)
            .await?
            .ok_or_else(|| SyncError::JobNotFound {
                job_id: job_id.to_string(),
            })
    }

    /// List sync history for a provider
    ///
    /// # Arguments
    ///
    /// * `provider` - Provider to query history for
    /// * `limit` - Maximum number of jobs to return
    ///
    /// # Returns
    ///
    /// Returns a list of sync jobs, most recent first
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let history = coordinator.list_history(ProviderKind::GoogleDrive, 10).await?;
    /// for job in history {
    ///     println!("Job {}: {:?}", job.id, job.status);
    /// }
    /// ```
    pub async fn list_history(
        &self,
        provider: ProviderKind,
        limit: usize,
    ) -> Result<Vec<SyncJob>> {
        self.job_repository.get_history(provider, limit.try_into().unwrap_or(u32::MAX)).await
    }

    /// Check if a sync is currently active for a profile
    ///
    /// # Arguments
    ///
    /// * `profile_id` - Profile to check
    ///
    /// # Returns
    ///
    /// Returns `true` if a sync is in progress for this profile
    pub async fn is_sync_active(&self, profile_id: ProfileId) -> bool {
        let active_syncs = self.active_syncs.lock().await;
        active_syncs.contains_key(&profile_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bridge_traits::error::BridgeError;
    use bytes::Bytes;
    use core_auth::AuthManager;
    use core_library::create_test_pool;
    use core_runtime::events::EventBus;

    // Mock storage provider for testing
    struct MockProvider {
        files: Vec<RemoteFile>,
    }

    #[async_trait::async_trait]
    impl StorageProvider for MockProvider {
        async fn list_media(
            &self,
            _cursor: Option<String>,
        ) -> bridge_traits::error::Result<(Vec<RemoteFile>, Option<String>)> {
            Ok((self.files.clone(), None))
        }

        async fn get_metadata(&self, _file_id: &str) -> bridge_traits::error::Result<RemoteFile> {
            Err(BridgeError::NotAvailable("get_metadata".to_string()))
        }

        async fn download(
            &self,
            _file_id: &str,
            _range: Option<&str>,
        ) -> bridge_traits::error::Result<Bytes> {
            Ok(Bytes::new())
        }

        async fn get_changes(
            &self,
            _cursor: Option<String>,
        ) -> bridge_traits::error::Result<(Vec<RemoteFile>, Option<String>)> {
            Ok((Vec::new(), None))
        }
    }

    async fn setup_test_coordinator() -> (SyncCoordinator, Arc<AuthManager>, SqlitePool) {
        let db_pool = create_test_pool().await.unwrap();

        // Create auth manager
        let event_bus = Arc::new(EventBus::new(100));
        
        // Create mock secure store and settings store
        use bridge_traits::storage::{SecureStore, SettingsStore};
        use std::collections::HashMap;
        use tokio::sync::Mutex as TokioMutex;

        struct MockSecureStore {
            data: Arc<TokioMutex<HashMap<String, Vec<u8>>>>,
        }

        #[async_trait::async_trait]
        impl SecureStore for MockSecureStore {
            async fn set_secret(&self, key: &str, value: &[u8]) -> bridge_traits::error::Result<()> {
                self.data.lock().await.insert(key.to_string(), value.to_vec());
                Ok(())
            }

            async fn get_secret(&self, key: &str) -> bridge_traits::error::Result<Option<Vec<u8>>> {
                Ok(self.data.lock().await.get(key).cloned())
            }

            async fn delete_secret(&self, key: &str) -> bridge_traits::error::Result<()> {
                self.data.lock().await.remove(key);
                Ok(())
            }

            async fn list_keys(&self) -> bridge_traits::error::Result<Vec<String>> {
                Ok(self.data.lock().await.keys().cloned().collect())
            }

            async fn clear_all(&self) -> bridge_traits::error::Result<()> {
                self.data.lock().await.clear();
                Ok(())
            }
        }

        struct MockHttpClient;

        #[async_trait::async_trait]
        impl bridge_traits::HttpClient for MockHttpClient {
            async fn execute(&self, _request: bridge_traits::HttpRequest) -> bridge_traits::error::Result<bridge_traits::HttpResponse> {
                Err(BridgeError::NotAvailable("http".to_string()))
            }

            async fn download_stream(
                &self,
                _url: String,
            ) -> bridge_traits::error::Result<Box<dyn tokio::io::AsyncRead + Send + Unpin>> {
                Err(BridgeError::NotAvailable("download_stream".to_string()))
            }
        }

        struct MockSettingsStore;

        #[async_trait::async_trait]
        impl SettingsStore for MockSettingsStore {
            async fn set_string(&self, _key: &str, _value: &str) -> bridge_traits::error::Result<()> {
                Ok(())
            }

            async fn get_string(&self, _key: &str) -> bridge_traits::error::Result<Option<String>> {
                Ok(None)
            }

            async fn set_bool(&self, _key: &str, _value: bool) -> bridge_traits::error::Result<()> {
                Ok(())
            }

            async fn get_bool(&self, _key: &str) -> bridge_traits::error::Result<Option<bool>> {
                Ok(None)
            }

            async fn set_i64(&self, _key: &str, _value: i64) -> bridge_traits::error::Result<()> {
                Ok(())
            }

            async fn get_i64(&self, _key: &str) -> bridge_traits::error::Result<Option<i64>> {
                Ok(None)
            }

            async fn set_f64(&self, _key: &str, _value: f64) -> bridge_traits::error::Result<()> {
                Ok(())
            }

            async fn get_f64(&self, _key: &str) -> bridge_traits::error::Result<Option<f64>> {
                Ok(None)
            }

            async fn delete(&self, _key: &str) -> bridge_traits::error::Result<()> {
                Ok(())
            }

            async fn has_key(&self, _key: &str) -> bridge_traits::error::Result<bool> {
                Ok(false)
            }

            async fn list_keys(&self) -> bridge_traits::error::Result<Vec<String>> {
                Ok(Vec::new())
            }

            async fn clear_all(&self) -> bridge_traits::error::Result<()> {
                Ok(())
            }

            async fn begin_transaction(
                &self,
            ) -> bridge_traits::error::Result<Box<dyn bridge_traits::storage::SettingsTransaction + Send>> {
                Err(BridgeError::NotAvailable("transactions".to_string()))
            }
        }

        let secure_store = Arc::new(MockSecureStore {
            data: Arc::new(TokioMutex::new(HashMap::new())),
        });
        let http_client = Arc::new(MockHttpClient);

        let auth_manager = Arc::new(
            AuthManager::new(
                secure_store,
                (*event_bus).clone(),
                http_client,
            )
        );

        let coordinator = SyncCoordinator::new(
            SyncConfig::default(),
            auth_manager.clone(),
            event_bus,
            None,
            db_pool.clone(),
        )
        .await
        .unwrap();

        (coordinator, auth_manager, db_pool)
    }

    #[tokio::test]
    async fn test_filter_audio_files() {
        let (coordinator, _, _) = setup_test_coordinator().await;

        let files = vec![
            RemoteFile {
                id: "1".to_string(),
                name: "song.mp3".to_string(),
                mime_type: Some("audio/mpeg".to_string()),
                size: Some(5_000_000),
                created_at: Some(1234567890),
                modified_at: Some(1234567890),
                is_folder: false,
                parent_ids: vec![],
                md5_checksum: None,
                metadata: Default::default(),
            },
            RemoteFile {
                id: "2".to_string(),
                name: "document.pdf".to_string(),
                mime_type: Some("application/pdf".to_string()),
                size: Some(100_000),
                created_at: Some(1234567890),
                modified_at: Some(1234567890),
                is_folder: false,
                parent_ids: vec![],
                md5_checksum: None,
                metadata: Default::default(),
            },
            RemoteFile {
                id: "3".to_string(),
                name: "album.flac".to_string(),
                mime_type: Some("audio/flac".to_string()),
                size: Some(30_000_000),
                created_at: Some(1234567890),
                modified_at: Some(1234567890),
                is_folder: false,
                parent_ids: vec![],
                md5_checksum: None,
                metadata: Default::default(),
            },
            RemoteFile {
                id: "4".to_string(),
                name: "Music".to_string(),
                mime_type: None,
                size: None,
                created_at: Some(1234567890),
                modified_at: Some(1234567890),
                is_folder: true,
                parent_ids: vec![],
                md5_checksum: None,
                metadata: Default::default(),
            },
        ];

        let audio_files = coordinator.filter_audio_files(files);
        assert_eq!(audio_files.len(), 2);
        assert_eq!(audio_files[0].id, "1");
        assert_eq!(audio_files[1].id, "3");
    }

    #[tokio::test]
    async fn test_register_provider() {
        let (coordinator, _, _) = setup_test_coordinator().await;

        let provider = Arc::new(MockProvider { files: vec![] });
        coordinator
            .register_provider(ProviderKind::GoogleDrive, provider)
            .await;

        let providers = coordinator.providers.read().await;
        assert!(providers.contains_key(&ProviderKind::GoogleDrive));
    }
}
