//! Folder repository trait and implementation

use crate::error::{LibraryError, Result};
use crate::models::Folder;
use crate::repositories::{Page, PageRequest};
use bridge_traits::database::{DatabaseAdapter, QueryRow, QueryValue};
use bridge_traits::platform::PlatformSendSync;
#[cfg(any(test, not(target_arch = "wasm32")))]
use sqlx::SqlitePool;
use std::sync::Arc;

/// Folder repository interface for data access operations
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
pub trait FolderRepository: PlatformSendSync {
    /// Find a folder by its ID
    ///
    /// # Returns
    /// - `Ok(Some(folder))` if found
    /// - `Ok(None)` if not found
    /// - `Err` if database error occurs
    async fn find_by_id(&self, id: &str) -> Result<Option<Folder>>;

    /// Insert a new folder
    ///
    /// # Errors
    /// Returns error if:
    /// - Folder with same ID already exists
    /// - Folder validation fails
    /// - Database error occurs
    async fn insert(&self, folder: &Folder) -> Result<()>;

    /// Update an existing folder
    ///
    /// # Errors
    /// Returns error if:
    /// - Folder does not exist
    /// - Folder validation fails
    /// - Database error occurs
    async fn update(&self, folder: &Folder) -> Result<()>;

    /// Delete a folder by ID
    ///
    /// # Returns
    /// - `Ok(true)` if folder was deleted
    /// - `Ok(false)` if folder was not found
    async fn delete(&self, id: &str) -> Result<bool>;

    /// Query folders with pagination
    ///
    /// # Arguments
    /// * `page_request` - Pagination parameters
    ///
    /// # Returns
    /// Paginated list of folders
    async fn query(&self, page_request: PageRequest) -> Result<Page<Folder>>;

    /// Query folders by provider with pagination
    ///
    /// # Arguments
    /// * `provider_id` - Provider identifier
    /// * `page_request` - Pagination parameters
    async fn query_by_provider(
        &self,
        provider_id: &str,
        page_request: PageRequest,
    ) -> Result<Page<Folder>>;

    /// Query child folders of a parent
    ///
    /// # Arguments
    /// * `parent_id` - Parent folder identifier (None for root folders)
    /// * `page_request` - Pagination parameters
    async fn query_children(
        &self,
        parent_id: Option<&str>,
        page_request: PageRequest,
    ) -> Result<Page<Folder>>;

    /// Find folder by path
    ///
    /// # Arguments
    /// * `provider_id` - Provider identifier
    /// * `path` - Folder path
    async fn find_by_path(&self, provider_id: &str, path: &str) -> Result<Option<Folder>>;

    /// Count total folders
    async fn count(&self) -> Result<i64>;
}

/// SQLite implementation of FolderRepository
pub struct SqliteFolderRepository {
    adapter: Arc<dyn DatabaseAdapter>,
}

impl SqliteFolderRepository {
    /// Create a new repository using the provided database adapter.
    pub fn new(adapter: Arc<dyn DatabaseAdapter>) -> Self {
        Self { adapter }
    }

    fn validate_folder(folder: &Folder) -> Result<()> {
        folder.validate().map_err(|msg| LibraryError::InvalidInput {
            field: "Folder".to_string(),
            message: msg,
        })
    }

    fn insert_params(folder: &Folder) -> Vec<QueryValue> {
        vec![
            QueryValue::Text(folder.id.clone()),
            QueryValue::Text(folder.provider_id.clone()),
            QueryValue::Text(folder.provider_folder_id.clone()),
            QueryValue::Text(folder.name.clone()),
            QueryValue::Text(folder.normalized_name.clone()),
            opt_text(&folder.parent_id),
            QueryValue::Text(folder.path.clone()),
            QueryValue::Integer(folder.created_at),
            QueryValue::Integer(folder.updated_at),
        ]
    }

    fn update_params(folder: &Folder) -> Vec<QueryValue> {
        let mut params = vec![
            QueryValue::Text(folder.name.clone()),
            QueryValue::Text(folder.normalized_name.clone()),
            opt_text(&folder.parent_id),
            QueryValue::Text(folder.path.clone()),
            QueryValue::Integer(folder.updated_at),
        ];
        params.push(QueryValue::Text(folder.id.clone()));
        params
    }

    async fn fetch_folders(&self, sql: &str, params: Vec<QueryValue>) -> Result<Vec<Folder>> {
        let rows = self.adapter.query(sql, &params).await?;
        rows.into_iter().map(|row| row_to_folder(&row)).collect()
    }

    async fn fetch_optional_folder(
        &self,
        sql: &str,
        params: Vec<QueryValue>,
    ) -> Result<Option<Folder>> {
        let row = self.adapter.query_one_optional(sql, &params).await?;
        row.map(|row| row_to_folder(&row)).transpose()
    }

    async fn paginate(
        &self,
        count_sql: &str,
        count_params: Vec<QueryValue>,
        data_sql: &str,
        mut data_params: Vec<QueryValue>,
        request: PageRequest,
    ) -> Result<Page<Folder>> {
        let total = self.count_with(count_sql, count_params).await?;
        data_params.push(QueryValue::Integer(request.limit() as i64));
        data_params.push(QueryValue::Integer(request.offset() as i64));
        let items = self.fetch_folders(data_sql, data_params).await?;
        Ok(Page::new(items, total as u64, request))
    }

    async fn count_with(&self, sql: &str, params: Vec<QueryValue>) -> Result<i64> {
        let row = self.adapter.query_one(sql, &params).await?;
        row.get("count")
            .and_then(|value| value.as_i64())
            .ok_or_else(|| missing_column("count"))
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl SqliteFolderRepository {
    /// Convenience constructor for native targets using an existing `sqlx` pool.
    pub fn from_pool(pool: SqlitePool) -> Self {
        use crate::adapters::sqlite_native::SqliteAdapter;
        Self::new(Arc::new(SqliteAdapter::from_pool(pool)))
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl FolderRepository for SqliteFolderRepository {
    async fn find_by_id(&self, id: &str) -> Result<Option<Folder>> {
        self.fetch_optional_folder(
            "SELECT * FROM folders WHERE id = ?",
            vec![QueryValue::Text(id.to_string())],
        )
        .await
    }

    async fn insert(&self, folder: &Folder) -> Result<()> {
        Self::validate_folder(folder)?;
        self.adapter
            .execute(
                r#"
                INSERT INTO folders (
                    id, provider_id, provider_folder_id, name, normalized_name, parent_id, path,
                    created_at, updated_at
                )
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
                "#,
                &Self::insert_params(folder),
            )
            .await?;
        Ok(())
    }

    async fn update(&self, folder: &Folder) -> Result<()> {
        Self::validate_folder(folder)?;
        let affected = self
            .adapter
            .execute(
                r#"
                UPDATE folders
                SET name = ?, normalized_name = ?, parent_id = ?, path = ?, updated_at = ?
                WHERE id = ?
                "#,
                &Self::update_params(folder),
            )
            .await?;
        if affected == 0 {
            return Err(LibraryError::NotFound {
                entity_type: "Folder".to_string(),
                id: folder.id.clone(),
            });
        }
        Ok(())
    }

    async fn delete(&self, id: &str) -> Result<bool> {
        let affected = self
            .adapter
            .execute(
                "DELETE FROM folders WHERE id = ?",
                &[QueryValue::Text(id.to_string())],
            )
            .await?;
        Ok(affected > 0)
    }

    async fn query(&self, page_request: PageRequest) -> Result<Page<Folder>> {
        self.paginate(
            "SELECT COUNT(*) as count FROM folders",
            vec![],
            "SELECT * FROM folders ORDER BY path ASC LIMIT ? OFFSET ?",
            vec![],
            page_request,
        )
        .await
    }

    async fn query_by_provider(
        &self,
        provider_id: &str,
        page_request: PageRequest,
    ) -> Result<Page<Folder>> {
        let params = vec![QueryValue::Text(provider_id.to_string())];
        self.paginate(
            "SELECT COUNT(*) as count FROM folders WHERE provider_id = ?",
            params.clone(),
            "SELECT * FROM folders WHERE provider_id = ? ORDER BY path ASC LIMIT ? OFFSET ?",
            params,
            page_request,
        )
        .await
    }

    async fn query_children(
        &self,
        parent_id: Option<&str>,
        page_request: PageRequest,
    ) -> Result<Page<Folder>> {
        match parent_id {
            Some(pid) => {
                let params = vec![QueryValue::Text(pid.to_string())];
                self.paginate(
                    "SELECT COUNT(*) as count FROM folders WHERE parent_id = ?",
                    params.clone(),
                    "SELECT * FROM folders WHERE parent_id = ? ORDER BY name ASC LIMIT ? OFFSET ?",
                    params,
                    page_request,
                )
                .await
            }
            None => {
                self.paginate(
                    "SELECT COUNT(*) as count FROM folders WHERE parent_id IS NULL",
                    vec![],
                    "SELECT * FROM folders WHERE parent_id IS NULL ORDER BY name ASC LIMIT ? OFFSET ?",
                    vec![],
                    page_request,
                )
                .await
            }
        }
    }

    async fn find_by_path(&self, provider_id: &str, path: &str) -> Result<Option<Folder>> {
        self.fetch_optional_folder(
            "SELECT * FROM folders WHERE provider_id = ? AND path = ? LIMIT 1",
            vec![
                QueryValue::Text(provider_id.to_string()),
                QueryValue::Text(path.to_string()),
            ],
        )
        .await
    }

    async fn count(&self) -> Result<i64> {
        self.count_with("SELECT COUNT(*) as count FROM folders", vec![])
            .await
    }
}

fn row_to_folder(row: &QueryRow) -> Result<Folder> {
    Ok(Folder {
        id: get_string(row, "id")?,
        provider_id: get_string(row, "provider_id")?,
        provider_folder_id: get_string(row, "provider_folder_id")?,
        name: get_string(row, "name")?,
        normalized_name: get_string(row, "normalized_name")?,
        parent_id: get_optional_string(row, "parent_id")?,
        path: get_string(row, "path")?,
        created_at: get_i64(row, "created_at")?,
        updated_at: get_i64(row, "updated_at")?,
    })
}

fn get_string(row: &QueryRow, key: &str) -> Result<String> {
    row.get(key)
        .and_then(|value| value.as_string())
        .ok_or_else(|| missing_column(key))
}

fn get_optional_string(row: &QueryRow, key: &str) -> Result<Option<String>> {
    Ok(match row.get(key) {
        Some(QueryValue::Null) | None => None,
        Some(value) => Some(value.as_string().ok_or_else(|| missing_column(key))?),
    })
}

fn get_i64(row: &QueryRow, key: &str) -> Result<i64> {
    row.get(key)
        .and_then(|value| value.as_i64())
        .ok_or_else(|| missing_column(key))
}

fn missing_column(column: &str) -> LibraryError {
    LibraryError::InvalidInput {
        field: column.to_string(),
        message: "missing column in result set".to_string(),
    }
}

fn opt_text(value: &Option<String>) -> QueryValue {
    value
        .as_ref()
        .map(|v| QueryValue::Text(v.clone()))
        .unwrap_or(QueryValue::Null)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::create_test_pool;

    async fn create_test_provider(pool: &SqlitePool, provider_id: &str) {
        sqlx::query(
            r#"
            INSERT OR IGNORE INTO providers (id, type, display_name, profile_id, created_at)
            VALUES (?, ?, ?, ?, ?)
            "#,
        )
        .bind(provider_id)
        .bind("GoogleDrive")
        .bind("Test Provider")
        .bind("test-profile")
        .bind(1699200000)
        .execute(pool)
        .await
        .unwrap();
    }

    fn create_test_folder(name: &str, provider_id: &str, parent_id: Option<String>) -> Folder {
        Folder {
            id: uuid::Uuid::new_v4().to_string(),
            provider_id: provider_id.to_string(),
            provider_folder_id: format!("prov-{}", name),
            name: name.to_string(),
            normalized_name: name.to_lowercase(),
            parent_id,
            path: format!("/{}", name),
            created_at: 1699200000,
            updated_at: 1699200000,
        }
    }

    #[core_async::test]
    async fn test_insert_and_find_folder() {
        let pool = create_test_pool().await.unwrap();
        let repo = SqliteFolderRepository::from_pool(pool.clone());

        // Create provider first
        create_test_provider(&pool, "provider1").await;

        // Create and insert folder
        let folder = create_test_folder("Music", "provider1", None);
        repo.insert(&folder).await.unwrap();

        // Find folder
        let found = repo.find_by_id(&folder.id).await.unwrap();
        assert!(found.is_some());
        let found = found.unwrap();
        assert_eq!(found.name, "Music");
        assert_eq!(found.updated_at, 1699200000);
    }

    #[core_async::test]
    async fn test_query_by_provider() {
        let pool = create_test_pool().await.unwrap();
        let repo = SqliteFolderRepository::from_pool(pool.clone());

        // Create providers first
        create_test_provider(&pool, "provider1").await;
        create_test_provider(&pool, "provider2").await;

        // Insert folders for different providers
        let folder1 = create_test_folder("Music", "provider1", None);
        let folder2 = create_test_folder("Videos", "provider1", None);
        let folder3 = create_test_folder("Documents", "provider2", None);

        repo.insert(&folder1).await.unwrap();
        repo.insert(&folder2).await.unwrap();
        repo.insert(&folder3).await.unwrap();

        // Query by provider1
        let page = repo
            .query_by_provider("provider1", PageRequest::default())
            .await
            .unwrap();

        assert_eq!(page.items.len(), 2);
        assert!(page.items.iter().all(|f| f.provider_id == "provider1"));
    }

    #[core_async::test]
    async fn test_query_children() {
        let pool = create_test_pool().await.unwrap();
        let repo = SqliteFolderRepository::from_pool(pool.clone());

        // Create provider first
        create_test_provider(&pool, "provider1").await;

        // Create parent folder
        let parent = create_test_folder("Music", "provider1", None);
        repo.insert(&parent).await.unwrap();

        // Create child folders
        let child1 = create_test_folder("Rock", "provider1", Some(parent.id.clone()));
        let child2 = create_test_folder("Jazz", "provider1", Some(parent.id.clone()));

        repo.insert(&child1).await.unwrap();
        repo.insert(&child2).await.unwrap();

        // Query children of parent
        let page = repo
            .query_children(Some(&parent.id), PageRequest::default())
            .await
            .unwrap();

        assert_eq!(page.items.len(), 2);
        assert!(page
            .items
            .iter()
            .all(|f| f.parent_id == Some(parent.id.clone())));
    }

    #[core_async::test]
    async fn test_find_by_path() {
        let pool = create_test_pool().await.unwrap();
        let repo = SqliteFolderRepository::from_pool(pool.clone());

        // Create provider first
        create_test_provider(&pool, "provider1").await;

        // Create and insert folder
        let folder = create_test_folder("Music", "provider1", None);
        repo.insert(&folder).await.unwrap();

        // Find by path
        let found = repo.find_by_path("provider1", "/Music").await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, folder.id);
    }

    #[core_async::test]
    async fn test_folder_validation() {
        let pool = create_test_pool().await.unwrap();
        let repo = SqliteFolderRepository::from_pool(pool);

        // Create folder with empty name
        let mut folder = create_test_folder("TestFolder", "provider1", None);
        folder.name = "".to_string();

        // Should fail validation
        let result = repo.insert(&folder).await;
        assert!(result.is_err());
    }
}
