//! Google Drive API response types
//!
//! Data structures for deserializing Google Drive API v3 responses.

use serde::{Deserialize, Serialize};

/// Google Drive API file resource
///
/// See: https://developers.google.com/drive/api/v3/reference/files#resource
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DriveFile {
    /// File ID
    pub id: String,

    /// File name
    pub name: String,

    /// MIME type
    pub mime_type: String,

    /// File size in bytes (omitted for folders)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<String>,

    /// Creation time (RFC 3339)
    pub created_time: String,

    /// Modification time (RFC 3339)
    pub modified_time: String,

    /// MD5 checksum (for files)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub md5_checksum: Option<String>,

    /// Parent folder IDs
    #[serde(default)]
    pub parents: Vec<String>,

    /// Whether file is trashed
    #[serde(default)]
    pub trashed: bool,
}

/// Google Drive API files.list response
///
/// See: https://developers.google.com/drive/api/v3/reference/files/list
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilesListResponse {
    /// List of files
    pub files: Vec<DriveFile>,

    /// Token for next page
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_page_token: Option<String>,

    /// Whether there are more results
    #[serde(default)]
    pub incomplete_search: bool,
}

/// Google Drive API changes.list response
///
/// See: https://developers.google.com/drive/api/v3/reference/changes/list
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangesListResponse {
    /// List of changes
    pub changes: Vec<Change>,

    /// Token for next page of changes
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_page_token: Option<String>,

    /// Token for watching for future changes
    pub new_start_page_token: String,
}

/// Google Drive API change resource
///
/// See: https://developers.google.com/drive/api/v3/reference/changes#resource
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Change {
    /// Type of change (file, drive)
    #[serde(rename = "type")]
    pub change_type: String,

    /// Time of change (RFC 3339)
    pub time: String,

    /// Whether file was removed
    pub removed: bool,

    /// File resource (if not removed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<DriveFile>,

    /// File ID (for removed files)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_id: Option<String>,
}

/// Google Drive API changes.getStartPageToken response
///
/// See: https://developers.google.com/drive/api/v3/reference/changes/getStartPageToken
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartPageTokenResponse {
    /// Token for first page of changes
    pub start_page_token: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_drive_file() {
        let json = r#"{
            "id": "abc123",
            "name": "test.mp3",
            "mimeType": "audio/mpeg",
            "size": "1024",
            "createdTime": "2023-01-01T00:00:00.000Z",
            "modifiedTime": "2023-01-02T00:00:00.000Z",
            "md5Checksum": "d41d8cd98f00b204e9800998ecf8427e",
            "parents": ["folder1"],
            "trashed": false
        }"#;

        let file: DriveFile = serde_json::from_str(json).unwrap();
        assert_eq!(file.id, "abc123");
        assert_eq!(file.name, "test.mp3");
        assert_eq!(file.mime_type, "audio/mpeg");
        assert_eq!(file.size, Some("1024".to_string()));
    }

    #[test]
    fn test_deserialize_files_list_response() {
        let json = r#"{
            "files": [
                {
                    "id": "file1",
                    "name": "song1.mp3",
                    "mimeType": "audio/mpeg",
                    "createdTime": "2023-01-01T00:00:00.000Z",
                    "modifiedTime": "2023-01-01T00:00:00.000Z",
                    "parents": []
                }
            ],
            "nextPageToken": "token123",
            "incompleteSearch": false
        }"#;

        let response: FilesListResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.files.len(), 1);
        assert_eq!(response.next_page_token, Some("token123".to_string()));
    }

    #[test]
    fn test_deserialize_changes_list_response() {
        let json = r#"{
            "changes": [
                {
                    "type": "file",
                    "time": "2023-01-01T00:00:00.000Z",
                    "removed": false,
                    "file": {
                        "id": "file1",
                        "name": "updated.mp3",
                        "mimeType": "audio/mpeg",
                        "createdTime": "2023-01-01T00:00:00.000Z",
                        "modifiedTime": "2023-01-02T00:00:00.000Z",
                        "parents": []
                    }
                }
            ],
            "newStartPageToken": "newtoken456"
        }"#;

        let response: ChangesListResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.changes.len(), 1);
        assert_eq!(response.new_start_page_token, "newtoken456");
        assert!(!response.changes[0].removed);
    }
}
