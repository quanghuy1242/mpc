//! Google Drive API connector implementation
//!
//! Implements the `StorageProvider` trait for Google Drive API v3.

use async_trait::async_trait;
use bridge_traits::error::Result;
use bridge_traits::http::HttpClient;
use bridge_traits::storage::{RemoteFile, StorageProvider};
use bytes::Bytes;
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info, instrument, warn};

use crate::error::GoogleDriveError;
use crate::types::{ChangesListResponse, DriveFile, FilesListResponse, StartPageTokenResponse};

/// Google Drive API base URL
const DRIVE_API_BASE: &str = "https://www.googleapis.com/drive/v3";

/// Maximum results per page (Google Drive API limit)
const MAX_PAGE_SIZE: u32 = 1000;

/// Fields to request for file resources
const FILE_FIELDS: &str = "id,name,mimeType,size,createdTime,modifiedTime,md5Checksum,parents,trashed";

/// Google Drive API connector
///
/// Implements `StorageProvider` for Google Drive API v3.
///
/// # Features
///
/// - Paginated file listing with audio MIME type filtering
/// - Streaming downloads with range request support
/// - Incremental sync using change tokens (pageToken)
/// - Exponential backoff for rate limiting
/// - OAuth 2.0 authentication via `HttpClient`
///
/// # Example
///
/// ```ignore
/// use provider_google_drive::GoogleDriveConnector;
/// use bridge_traits::storage::StorageProvider;
///
/// let connector = GoogleDriveConnector::new(http_client, access_token);
/// let (files, next_cursor) = connector.list_media(None).await?;
/// ```
pub struct GoogleDriveConnector {
    /// HTTP client for API requests
    http_client: Arc<dyn HttpClient>,

    /// OAuth 2.0 access token
    access_token: String,
}

impl GoogleDriveConnector {
    /// Create a new Google Drive connector
    ///
    /// # Arguments
    ///
    /// * `http_client` - HTTP client implementation
    /// * `access_token` - OAuth 2.0 access token with `drive.readonly` scope
    pub fn new(http_client: Arc<dyn HttpClient>, access_token: String) -> Self {
        Self {
            http_client,
            access_token,
        }
    }

    /// Build authorization header value
    fn auth_header(&self) -> String {
        format!("Bearer {}", self.access_token)
    }

    /// Parse RFC 3339 timestamp to Unix timestamp
    fn parse_timestamp(rfc3339: &str) -> Option<i64> {
        DateTime::parse_from_rfc3339(rfc3339)
            .ok()
            .map(|dt| dt.with_timezone(&Utc).timestamp())
    }

    /// Convert DriveFile to RemoteFile
    fn convert_file(&self, drive_file: DriveFile) -> RemoteFile {
        let mut metadata = HashMap::new();
        metadata.insert("trashed".to_string(), drive_file.trashed.to_string());

        RemoteFile {
            id: drive_file.id,
            name: drive_file.name,
            mime_type: Some(drive_file.mime_type.clone()),
            size: drive_file.size.and_then(|s| s.parse().ok()),
            created_at: Self::parse_timestamp(&drive_file.created_time),
            modified_at: Self::parse_timestamp(&drive_file.modified_time),
            is_folder: drive_file.mime_type == "application/vnd.google-apps.folder",
            parent_ids: drive_file.parents,
            md5_checksum: drive_file.md5_checksum,
            metadata,
        }
    }

    /// Execute API request with retry logic
    ///
    /// Implements exponential backoff for rate limiting and transient errors.
    #[instrument(skip(self), fields(url = %url))]
    async fn execute_with_retry(
        &self,
        url: String,
        max_retries: u32,
    ) -> Result<bridge_traits::http::HttpResponse> {
        let mut attempt = 0;

        loop {
            let mut headers = HashMap::new();
            headers.insert("Authorization".to_string(), self.auth_header());
            headers.insert("Accept".to_string(), "application/json".to_string());
            
            let request = bridge_traits::http::HttpRequest {
                method: bridge_traits::http::HttpMethod::Get,
                url: url.clone(),
                headers,
                body: None,
                timeout: Some(std::time::Duration::from_secs(30)),
            };

            match self.http_client.execute(request).await {
                Ok(response) => {
                    let status = response.status;

                    if status == 200 {
                        debug!("API request succeeded: status={}", status);
                        return Ok(response);
                    } else if status == 429 || (status >= 500 && status < 600) {
                        // Rate limit or server error - retry with backoff
                        attempt += 1;
                        if attempt >= max_retries {
                            warn!(
                                "API request failed after {} attempts: status={}",
                                max_retries, status
                            );
                            return Err(GoogleDriveError::ApiError {
                                status_code: status,
                                message: format!("Request failed after {} retries", max_retries),
                            }
                            .into());
                        }

                        let backoff_ms = 100u64 * 2u64.pow(attempt);
                        warn!(
                            "API request failed (attempt {}/{}): status={}, retrying in {}ms",
                            attempt, max_retries, status, backoff_ms
                        );
                        tokio::time::sleep(tokio::time::Duration::from_millis(backoff_ms)).await;
                    } else {
                        // Client error - don't retry
                        warn!("API request failed: status={}", status);
                        return Err(GoogleDriveError::ApiError {
                            status_code: status,
                            message: String::from_utf8_lossy(&response.body).to_string(),
                        }
                        .into());
                    }
                }
                Err(e) => {
                    attempt += 1;
                    if attempt >= max_retries {
                        warn!("API request failed after {} attempts: {}", max_retries, e);
                        return Err(e);
                    }

                    let backoff_ms = 100u64 * 2u64.pow(attempt);
                    warn!(
                        "API request failed (attempt {}/{}): {}, retrying in {}ms",
                        attempt, max_retries, e, backoff_ms
                    );
                    tokio::time::sleep(tokio::time::Duration::from_millis(backoff_ms)).await;
                }
            }
        }
    }
}

#[async_trait]
impl StorageProvider for GoogleDriveConnector {
    #[instrument(skip(self))]
    async fn list_media(&self, cursor: Option<String>) -> Result<(Vec<RemoteFile>, Option<String>)> {
        info!("Listing files from Google Drive");

        // Build query: not trashed
        let query = "trashed=false";

        // Build URL with pagination
        let mut url = format!(
            "{}/files?q={}&pageSize={}&fields=nextPageToken,incompleteSearch,files({})",
            DRIVE_API_BASE,
            urlencoding::encode(query),
            MAX_PAGE_SIZE,
            FILE_FIELDS
        );

        if let Some(page_token) = cursor {
            url.push_str(&format!("&pageToken={}", urlencoding::encode(&page_token)));
        }

        // Execute request
        let response = self.execute_with_retry(url, 3).await?;

        // Parse response body directly (it's already Bytes)
        let list_response: FilesListResponse = serde_json::from_slice(&response.body).map_err(|e| {
            GoogleDriveError::ParseError(format!("Failed to parse files list response: {}", e))
        })?;

        // Convert files
        let files: Vec<RemoteFile> = list_response
            .files
            .into_iter()
            .map(|f| self.convert_file(f))
            .collect();

        info!("Listed {} files from Google Drive", files.len());

        Ok((files, list_response.next_page_token))
    }

    #[instrument(skip(self), fields(file_id = %file_id))]
    async fn get_metadata(&self, file_id: &str) -> Result<RemoteFile> {
        info!("Getting metadata for file: {}", file_id);

        let url = format!(
            "{}/files/{}?fields={}",
            DRIVE_API_BASE, file_id, FILE_FIELDS
        );

        let response = self.execute_with_retry(url, 3).await?;

        // Parse response body directly
        let drive_file: DriveFile = serde_json::from_slice(&response.body).map_err(|e| {
            GoogleDriveError::ParseError(format!("Failed to parse file metadata: {}", e))
        })?;

        Ok(self.convert_file(drive_file))
    }

    #[instrument(skip(self), fields(file_id = %file_id, range = ?range))]
    async fn download(&self, file_id: &str, range: Option<&str>) -> Result<Bytes> {
        info!("Downloading file: {}", file_id);

        let url = format!("{}/files/{}?alt=media", DRIVE_API_BASE, file_id);

        let mut headers = HashMap::new();
        headers.insert("Authorization".to_string(), self.auth_header());

        if let Some(range_value) = range {
            headers.insert("Range".to_string(), range_value.to_string());
        }

        let request = bridge_traits::http::HttpRequest {
            method: bridge_traits::http::HttpMethod::Get,
            url,
            headers,
            body: None,
            timeout: Some(std::time::Duration::from_secs(60)),
        };

        let response = self.http_client.execute(request).await?;

        if response.status == 200 || response.status == 206 {
            info!("Downloaded {} bytes", response.body.len());
            Ok(Bytes::from(response.body))
        } else {
            Err(GoogleDriveError::ApiError {
                status_code: response.status,
                message: String::from_utf8_lossy(&response.body).to_string(),
            }
            .into())
        }
    }

    #[instrument(skip(self))]
    async fn get_changes(&self, cursor: Option<String>) -> Result<(Vec<RemoteFile>, Option<String>)> {
        info!("Getting changes from Google Drive");

        // If no cursor provided, get start page token
        let page_token = if let Some(token) = cursor {
            token
        } else {
            let url = format!("{}/changes/startPageToken", DRIVE_API_BASE);
            let response = self.execute_with_retry(url, 3).await?;

            // Parse start page token directly
            let start_token: StartPageTokenResponse =
                serde_json::from_slice(&response.body).map_err(|e| {
                    GoogleDriveError::ParseError(format!("Failed to parse start page token: {}", e))
                })?;

            start_token.start_page_token
        };

        // Get changes
        let url = format!(
            "{}/changes?pageToken={}&fields=nextPageToken,newStartPageToken,changes(type,time,removed,file({}),fileId)",
            DRIVE_API_BASE,
            urlencoding::encode(&page_token),
            FILE_FIELDS
        );

        let response = self.execute_with_retry(url, 3).await?;

        // Parse changes list directly
        let changes_list: ChangesListResponse = serde_json::from_slice(&response.body).map_err(|e| {
            GoogleDriveError::ParseError(format!("Failed to parse changes list: {}", e))
        })?;

        // Convert changes to files
        let files: Vec<RemoteFile> = changes_list
            .changes
            .into_iter()
            .filter_map(|change| {
                if change.removed {
                    // For removed files, we can't provide full metadata
                    // Caller should handle this based on file_id in metadata
                    change.file_id.map(|id| RemoteFile {
                        id,
                        name: String::new(),
                        mime_type: None,
                        size: None,
                        created_at: None,
                        modified_at: None,
                        is_folder: false,
                        parent_ids: vec![],
                        md5_checksum: None,
                        metadata: {
                            let mut map = HashMap::new();
                            map.insert("removed".to_string(), "true".to_string());
                            map
                        },
                    })
                } else {
                    change.file.map(|f| self.convert_file(f))
                }
            })
            .collect();

        let next_cursor = changes_list
            .next_page_token
            .or(Some(changes_list.new_start_page_token));

        info!("Retrieved {} changes from Google Drive", files.len());

        Ok((files, next_cursor))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockall::mock;

    mock! {
        HttpClient {}

        #[async_trait]
        impl HttpClient for HttpClient {
            async fn execute(&self, request: bridge_traits::http::HttpRequest) -> Result<bridge_traits::http::HttpResponse>;
            async fn download_stream(&self, url: String) -> Result<Box<dyn tokio::io::AsyncRead + Send + Unpin>>;
        }
    }

    #[tokio::test]
    async fn test_convert_file() {
        let http_client = Arc::new(MockHttpClient::new());
        let connector = GoogleDriveConnector::new(http_client, "test_token".to_string());

        let drive_file = DriveFile {
            id: "file123".to_string(),
            name: "test.mp3".to_string(),
            mime_type: "audio/mpeg".to_string(),
            size: Some("1024".to_string()),
            created_time: "2023-01-01T00:00:00.000Z".to_string(),
            modified_time: "2023-01-02T00:00:00.000Z".to_string(),
            md5_checksum: Some("abc123".to_string()),
            parents: vec!["folder1".to_string()],
            trashed: false,
        };

        let remote_file = connector.convert_file(drive_file);

        assert_eq!(remote_file.id, "file123");
        assert_eq!(remote_file.name, "test.mp3");
        assert_eq!(remote_file.mime_type, Some("audio/mpeg".to_string()));
        assert_eq!(remote_file.size, Some(1024));
        assert!(!remote_file.is_folder);
        assert_eq!(remote_file.md5_checksum, Some("abc123".to_string()));
    }

    #[tokio::test]
    async fn test_convert_folder() {
        let http_client = Arc::new(MockHttpClient::new());
        let connector = GoogleDriveConnector::new(http_client, "test_token".to_string());

        let drive_folder = DriveFile {
            id: "folder123".to_string(),
            name: "Music".to_string(),
            mime_type: "application/vnd.google-apps.folder".to_string(),
            size: None,
            created_time: "2023-01-01T00:00:00.000Z".to_string(),
            modified_time: "2023-01-02T00:00:00.000Z".to_string(),
            md5_checksum: None,
            parents: vec![],
            trashed: false,
        };

        let remote_file = connector.convert_file(drive_folder);

        assert!(remote_file.is_folder);
        assert_eq!(remote_file.size, None);
    }

    #[tokio::test]
    async fn test_list_media_success() {
        let mut mock_http = MockHttpClient::new();
        
        mock_http.expect_execute()
            .times(1)
            .returning(|_| {
                let response_body = r#"{
                    "files": [
                        {
                            "id": "file1",
                            "name": "song.mp3",
                            "mimeType": "audio/mpeg",
                            "size": "1024",
                            "createdTime": "2024-01-01T00:00:00.000Z",
                            "modifiedTime": "2024-01-01T00:00:00.000Z",
                            "parents": ["parent1"],
                            "md5Checksum": "abc123",
                            "trashed": false
                        }
                    ],
                    "nextPageToken": "next_page"
                }"#;
                
                Ok(bridge_traits::http::HttpResponse {
                    status: 200,
                    headers: HashMap::new(),
                    body: Bytes::from(response_body.as_bytes()),
                })
            });

        let connector = GoogleDriveConnector::new(Arc::new(mock_http), "test_token".to_string());
        let (files, cursor) = connector.list_media(None).await.unwrap();

        assert_eq!(files.len(), 1);
        assert_eq!(files[0].id, "file1");
        assert_eq!(files[0].name, "song.mp3");
        assert_eq!(cursor, Some("next_page".to_string()));
    }

    #[tokio::test]
    async fn test_get_metadata_success() {
        let mut mock_http = MockHttpClient::new();
        
        mock_http.expect_execute()
            .times(1)
            .returning(|_| {
                let response_body = r#"{
                    "id": "file1",
                    "name": "song.mp3",
                    "mimeType": "audio/mpeg",
                    "size": "1024",
                    "createdTime": "2024-01-01T00:00:00.000Z",
                    "modifiedTime": "2024-01-01T00:00:00.000Z",
                    "parents": ["parent1"],
                    "md5Checksum": "abc123",
                    "trashed": false
                }"#;
                
                Ok(bridge_traits::http::HttpResponse {
                    status: 200,
                    headers: HashMap::new(),
                    body: Bytes::from(response_body.as_bytes()),
                })
            });

        let connector = GoogleDriveConnector::new(Arc::new(mock_http), "test_token".to_string());
        let file = connector.get_metadata("file1").await.unwrap();

        assert_eq!(file.id, "file1");
        assert_eq!(file.name, "song.mp3");
        assert_eq!(file.mime_type, Some("audio/mpeg".to_string()));
    }

    #[tokio::test]
    async fn test_download_success() {
        let mut mock_http = MockHttpClient::new();
        
        mock_http.expect_execute()
            .times(1)
            .returning(|req| {
                // Verify authorization header
                assert!(req.headers.contains_key("Authorization"));
                assert!(req.url.contains("alt=media"));
                
                Ok(bridge_traits::http::HttpResponse {
                    status: 200,
                    headers: HashMap::new(),
                    body: Bytes::from(vec![1, 2, 3, 4, 5]),
                })
            });

        let connector = GoogleDriveConnector::new(Arc::new(mock_http), "test_token".to_string());
        let data = connector.download("file1", None).await.unwrap();

        assert_eq!(data.len(), 5);
        assert_eq!(&data[..], &[1, 2, 3, 4, 5]);
    }

    #[tokio::test]
    async fn test_download_with_range() {
        let mut mock_http = MockHttpClient::new();
        
        mock_http.expect_execute()
            .times(1)
            .returning(|req| {
                // Verify Range header exists
                assert!(req.headers.contains_key("Range"));
                
                Ok(bridge_traits::http::HttpResponse {
                    status: 206,  // Partial Content
                    headers: HashMap::new(),
                    body: Bytes::from(vec![1, 2, 3]),
                })
            });

        let connector = GoogleDriveConnector::new(Arc::new(mock_http), "test_token".to_string());
        let data = connector.download("file1", Some("bytes=0-2")).await.unwrap();

        assert_eq!(data.len(), 3);
    }

    #[tokio::test]
    async fn test_get_changes_with_token() {
        let mut mock_http = MockHttpClient::new();
        
        mock_http.expect_execute()
            .times(1)
            .returning(|_| {
                let response_body = r#"{
                    "changes": [
                        {
                            "type": "file",
                            "time": "2024-01-01T00:00:00.000Z",
                            "fileId": "file1",
                            "removed": false,
                            "file": {
                                "id": "file1",
                                "name": "song.mp3",
                                "mimeType": "audio/mpeg",
                                "size": "1024",
                                "createdTime": "2024-01-01T00:00:00.000Z",
                                "modifiedTime": "2024-01-01T00:00:00.000Z",
                                "parents": [],
                                "trashed": false
                            }
                        }
                    ],
                    "newStartPageToken": "token456"
                }"#;
                
                Ok(bridge_traits::http::HttpResponse {
                    status: 200,
                    headers: HashMap::new(),
                    body: Bytes::from(response_body.as_bytes()),
                })
            });

        let connector = GoogleDriveConnector::new(Arc::new(mock_http), "test_token".to_string());
        let (files, cursor) = connector.get_changes(Some("token123".to_string())).await.unwrap();

        assert_eq!(files.len(), 1);
        assert_eq!(files[0].id, "file1");
        assert_eq!(cursor, Some("token456".to_string()));
    }

    #[tokio::test]
    async fn test_get_changes_removed_file() {
        let mut mock_http = MockHttpClient::new();
        
        mock_http.expect_execute()
            .times(1)
            .returning(|_| {
                let response_body = r#"{
                    "changes": [
                        {
                            "type": "file",
                            "time": "2024-01-01T00:00:00.000Z",
                            "fileId": "file1",
                            "removed": true
                        }
                    ],
                    "newStartPageToken": "token456"
                }"#;
                
                Ok(bridge_traits::http::HttpResponse {
                    status: 200,
                    headers: HashMap::new(),
                    body: Bytes::from(response_body.as_bytes()),
                })
            });

        let connector = GoogleDriveConnector::new(Arc::new(mock_http), "test_token".to_string());
        let (files, _) = connector.get_changes(Some("token123".to_string())).await.unwrap();

        assert_eq!(files.len(), 1);
        assert_eq!(files[0].id, "file1");
        assert_eq!(files[0].metadata.get("removed"), Some(&"true".to_string()));
    }

    #[tokio::test]
    async fn test_api_error_handling() {
        let mut mock_http = MockHttpClient::new();
        
        mock_http.expect_execute()
            .times(1)
            .returning(|_| {
                Ok(bridge_traits::http::HttpResponse {
                    status: 404,
                    headers: HashMap::new(),
                    body: Bytes::from(b"File not found".to_vec()),
                })
            });

        let connector = GoogleDriveConnector::new(Arc::new(mock_http), "test_token".to_string());
        let result = connector.get_metadata("nonexistent").await;

        assert!(result.is_err());
    }
}
