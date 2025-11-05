//! Pagination helper types for repository queries

use serde::{Deserialize, Serialize};

/// Pagination request parameters
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageRequest {
    /// Current page number (0-indexed)
    pub page: u32,
    /// Number of items per page
    pub page_size: u32,
}

impl PageRequest {
    /// Create a new page request
    ///
    /// # Examples
    ///
    /// ```
    /// use core_library::repositories::PageRequest;
    ///
    /// let request = PageRequest::new(0, 20);
    /// assert_eq!(request.page, 0);
    /// assert_eq!(request.page_size, 20);
    /// ```
    pub fn new(page: u32, page_size: u32) -> Self {
        Self { page, page_size }
    }

    /// Calculate the SQL OFFSET value
    pub fn offset(&self) -> u32 {
        self.page * self.page_size
    }

    /// Get the LIMIT value (same as page_size)
    pub fn limit(&self) -> u32 {
        self.page_size
    }
}

impl Default for PageRequest {
    fn default() -> Self {
        Self {
            page: 0,
            page_size: 50,
        }
    }
}

/// Paginated response containing items and metadata
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Page<T> {
    /// Items in the current page
    pub items: Vec<T>,
    /// Total number of items across all pages
    pub total: u64,
    /// Current page number
    pub page: u32,
    /// Total number of pages
    pub total_pages: u32,
    /// Number of items per page
    pub page_size: u32,
}

impl<T> Page<T> {
    /// Create a new paginated response
    ///
    /// # Examples
    ///
    /// ```
    /// use core_library::repositories::{Page, PageRequest};
    ///
    /// let items = vec![1, 2, 3];
    /// let request = PageRequest::new(0, 10);
    /// let page = Page::new(items, 25, request);
    ///
    /// assert_eq!(page.items.len(), 3);
    /// assert_eq!(page.total, 25);
    /// assert_eq!(page.page, 0);
    /// assert_eq!(page.total_pages, 3);
    /// ```
    pub fn new(items: Vec<T>, total: u64, request: PageRequest) -> Self {
        let total_pages = if request.page_size == 0 {
            0
        } else {
            ((total as f64) / (request.page_size as f64)).ceil() as u32
        };

        Self {
            items,
            total,
            page: request.page,
            total_pages,
            page_size: request.page_size,
        }
    }

    /// Check if there are more pages after the current one
    pub fn has_next(&self) -> bool {
        self.page + 1 < self.total_pages
    }

    /// Check if there are pages before the current one
    pub fn has_previous(&self) -> bool {
        self.page > 0
    }

    /// Map the items to a different type
    pub fn map<U, F>(self, f: F) -> Page<U>
    where
        F: FnMut(T) -> U,
    {
        Page {
            items: self.items.into_iter().map(f).collect(),
            total: self.total,
            page: self.page,
            total_pages: self.total_pages,
            page_size: self.page_size,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_page_request_default() {
        let request = PageRequest::default();
        assert_eq!(request.page, 0);
        assert_eq!(request.page_size, 50);
    }

    #[test]
    fn test_page_request_offset() {
        let request = PageRequest::new(0, 20);
        assert_eq!(request.offset(), 0);

        let request = PageRequest::new(2, 20);
        assert_eq!(request.offset(), 40);
    }

    #[test]
    fn test_page_request_limit() {
        let request = PageRequest::new(0, 20);
        assert_eq!(request.limit(), 20);
    }

    #[test]
    fn test_page_new() {
        let items = vec![1, 2, 3];
        let request = PageRequest::new(0, 10);
        let page = Page::new(items, 25, request);

        assert_eq!(page.items.len(), 3);
        assert_eq!(page.total, 25);
        assert_eq!(page.page, 0);
        assert_eq!(page.total_pages, 3);
        assert_eq!(page.page_size, 10);
    }

    #[test]
    fn test_page_has_next() {
        let page = Page::new(vec![1, 2, 3], 25, PageRequest::new(0, 10));
        assert!(page.has_next());

        let page = Page::new(vec![1, 2, 3], 25, PageRequest::new(2, 10));
        assert!(!page.has_next());
    }

    #[test]
    fn test_page_has_previous() {
        let page = Page::new(vec![1, 2, 3], 25, PageRequest::new(0, 10));
        assert!(!page.has_previous());

        let page = Page::new(vec![1, 2, 3], 25, PageRequest::new(1, 10));
        assert!(page.has_previous());
    }

    #[test]
    fn test_page_map() {
        let page = Page::new(vec![1, 2, 3], 25, PageRequest::new(0, 10));
        let mapped = page.map(|x| x * 2);

        assert_eq!(mapped.items, vec![2, 4, 6]);
        assert_eq!(mapped.total, 25);
        assert_eq!(mapped.page, 0);
    }

    #[test]
    fn test_page_zero_page_size() {
        let page = Page::new(vec![1, 2, 3], 25, PageRequest::new(0, 0));
        assert_eq!(page.total_pages, 0);
    }
}
