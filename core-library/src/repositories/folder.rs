//! Folder repository trait and implementation

use crate::error::{LibraryError, Result};
use crate::models::Folder;
use crate::repositories::{Page, PageRequest};
use async_trait::async_trait;
use sqlx::{query, query_as, SqlitePool};

/// Folder repository interface for data access operations
#[async_trait]
pub trait FolderRepository: Send + Sync {
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
    pool: SqlitePool,
}

impl SqliteFolderRepository {
    /// Create a new SqliteFolderRepository
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl FolderRepository for SqliteFolderRepository {
    async fn find_by_id(&self, id: &str) -> Result<Option<Folder>> {
        let folder = query_as::<_, Folder>("SELECT * FROM folders WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;

        Ok(folder)
    }

    async fn insert(&self, folder: &Folder) -> Result<()> {
        // Validate before insertion
        folder.validate().map_err(|e| LibraryError::InvalidInput {
            field: "Folder".to_string(),
            message: e,
        })?;

        query(
            r#"
            INSERT INTO folders (
                id, provider_id, provider_folder_id, name, normalized_name, parent_id, path,
                created_at, updated_at
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&folder.id)
        .bind(&folder.provider_id)
        .bind(&folder.provider_folder_id)
        .bind(&folder.name)
        .bind(&folder.normalized_name)
        .bind(&folder.parent_id)
        .bind(&folder.path)
        .bind(folder.created_at)
        .bind(folder.updated_at)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn update(&self, folder: &Folder) -> Result<()> {
        // Validate before update
        folder.validate().map_err(|e| LibraryError::InvalidInput {
            field: "Folder".to_string(),
            message: e,
        })?;

        let result = query(
            r#"
            UPDATE folders
            SET name = ?, normalized_name = ?, parent_id = ?, path = ?, updated_at = ?
            WHERE id = ?
            "#,
        )
        .bind(&folder.name)
        .bind(&folder.normalized_name)
        .bind(&folder.parent_id)
        .bind(&folder.path)
        .bind(folder.updated_at)
        .bind(&folder.id)
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(LibraryError::NotFound {
                entity_type: "Folder".to_string(),
                id: folder.id.clone(),
            });
        }

        Ok(())
    }

    async fn delete(&self, id: &str) -> Result<bool> {
        let result = query("DELETE FROM folders WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    async fn query(&self, page_request: PageRequest) -> Result<Page<Folder>> {
        let total = self.count().await?;

        let folders =
            query_as::<_, Folder>("SELECT * FROM folders ORDER BY path ASC LIMIT ? OFFSET ?")
                .bind(page_request.limit())
                .bind(page_request.offset())
                .fetch_all(&self.pool)
                .await?;

        Ok(Page::new(folders, total as u64, page_request))
    }

    async fn query_by_provider(
        &self,
        provider_id: &str,
        page_request: PageRequest,
    ) -> Result<Page<Folder>> {
        let total: i64 = query_as("SELECT COUNT(*) as count FROM folders WHERE provider_id = ?")
            .bind(provider_id)
            .fetch_one(&self.pool)
            .await
            .map(|row: (i64,)| row.0)?;

        let folders = query_as::<_, Folder>(
            "SELECT * FROM folders WHERE provider_id = ? ORDER BY path ASC LIMIT ? OFFSET ?",
        )
        .bind(provider_id)
        .bind(page_request.limit())
        .bind(page_request.offset())
        .fetch_all(&self.pool)
        .await?;

        Ok(Page::new(folders, total as u64, page_request))
    }

    async fn query_children(
        &self,
        parent_id: Option<&str>,
        page_request: PageRequest,
    ) -> Result<Page<Folder>> {
        let (total, folders) = match parent_id {
            Some(pid) => {
                let total: i64 =
                    query_as("SELECT COUNT(*) as count FROM folders WHERE parent_id = ?")
                        .bind(pid)
                        .fetch_one(&self.pool)
                        .await
                        .map(|row: (i64,)| row.0)?;

                let folders = query_as::<_, Folder>(
                    "SELECT * FROM folders WHERE parent_id = ? ORDER BY name ASC LIMIT ? OFFSET ?",
                )
                .bind(pid)
                .bind(page_request.limit())
                .bind(page_request.offset())
                .fetch_all(&self.pool)
                .await?;

                (total, folders)
            }
            None => {
                let total: i64 =
                    query_as("SELECT COUNT(*) as count FROM folders WHERE parent_id IS NULL")
                        .fetch_one(&self.pool)
                        .await
                        .map(|row: (i64,)| row.0)?;

                let folders = query_as::<_, Folder>(
                    "SELECT * FROM folders WHERE parent_id IS NULL ORDER BY name ASC LIMIT ? OFFSET ?",
                )
                .bind(page_request.limit())
                .bind(page_request.offset())
                .fetch_all(&self.pool)
                .await
                ?;

                (total, folders)
            }
        };

        Ok(Page::new(folders, total as u64, page_request))
    }

    async fn find_by_path(&self, provider_id: &str, path: &str) -> Result<Option<Folder>> {
        let folder = query_as::<_, Folder>(
            "SELECT * FROM folders WHERE provider_id = ? AND path = ? LIMIT 1",
        )
        .bind(provider_id)
        .bind(path)
        .fetch_optional(&self.pool)
        .await?;

        Ok(folder)
    }

    async fn count(&self) -> Result<i64> {
        let count: i64 = query_as("SELECT COUNT(*) as count FROM folders")
            .fetch_one(&self.pool)
            .await
            .map(|row: (i64,)| row.0)?;

        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::create_test_pool;

    async fn setup_test_pool() -> SqlitePool {
        create_test_pool().await.unwrap()
    }

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

    #[tokio::test]
    async fn test_insert_and_find_folder() {
        let pool = setup_test_pool().await;
        let repo = SqliteFolderRepository::new(pool.clone());

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

    #[tokio::test]
    async fn test_query_by_provider() {
        let pool = setup_test_pool().await;
        let repo = SqliteFolderRepository::new(pool.clone());

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

    #[tokio::test]
    async fn test_query_children() {
        let pool = setup_test_pool().await;
        let repo = SqliteFolderRepository::new(pool.clone());

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

    #[tokio::test]
    async fn test_find_by_path() {
        let pool = setup_test_pool().await;
        let repo = SqliteFolderRepository::new(pool.clone());

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

    #[tokio::test]
    async fn test_folder_validation() {
        let pool = setup_test_pool().await;
        let repo = SqliteFolderRepository::new(pool);

        // Create folder with empty name
        let mut folder = create_test_folder("TestFolder", "provider1", None);
        folder.name = "".to_string();

        // Should fail validation
        let result = repo.insert(&folder).await;
        assert!(result.is_err());
    }
}
