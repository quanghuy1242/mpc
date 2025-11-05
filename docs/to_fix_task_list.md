# Tasks Requiring Follow-Up

Verification against `docs/core_architecture.md` surfaced several gaps in tasks marked ✅ in `docs/ai_task_list.md`. Addressing these will keep the implementation aligned with the documented architecture.

## TASK-005 – Create Core Configuration System
- ✅ Default desktop configuration now initializes `SecureStore` and `SettingsStore` automatically via background runtime bootstrap (`core-runtime/src/config.rs`). Added regression tests to cover building inside and outside Tokio runtimes.

## TASK-102 – Implement OAuth 2.0 Flow Manager
- ✅ `OAuthFlowManager` now relies on the injected `HttpClient` trait for code exchange and refresh logic with retries (`core-auth/src/oauth.rs`). `AuthManager::new` requires the host HTTP client, keeping platform networking under trait control.
