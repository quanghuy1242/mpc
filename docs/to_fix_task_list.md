# Tasks Requiring Follow-Up

Verification against `docs/core_architecture.md` surfaced several gaps in tasks marked ✅ in `docs/ai_task_list.md`. Addressing these will keep the implementation aligned with the documented architecture.

## TASK-005 – Create Core Configuration System
- ✅ Default desktop configuration now initializes `SecureStore` and `SettingsStore` automatically via background runtime bootstrap (`core-runtime/src/config.rs`). Added regression tests to cover building inside and outside Tokio runtimes.

## TASK-102 – Implement OAuth 2.0 Flow Manager
- `OAuthFlowManager::exchange_code` constructs a `reqwest::Client` directly (`core-auth/src/oauth.rs:325-330`), bypassing the host-provided `HttpClient` abstraction mandated for portability (`docs/core_architecture.md:25-33`). Pass an injected `HttpClient` (or reuse one from `CoreConfig`) so mobile/Web hosts can supply their own stacks.
