# Tasks Requiring Follow-Up

Verification against `docs/core_architecture.md` surfaced several gaps in tasks marked ✅ in `docs/ai_task_list.md`. Addressing these will keep the implementation aligned with the documented architecture.

## TASK-002 – Define Host Bridge Traits
- The architecture requires audio routing traits (`PlaybackAdapter`, `AudioDecoder`) to keep playback pluggable (`docs/core_architecture.md:210-219`), but `bridge-traits/src/lib.rs:13` only exposes networking/storage/platform utilities. Add the missing playback traits (and related types) so downstream modules can compile against the documented contract.

## TASK-003 – Implement Desktop Bridge Shims
- `TokioBackgroundExecutor` currently just records tasks without executing them or honoring constraints (`bridge-desktop/src/background.rs:63-118`), conflicting with the requirement for platform-aware scheduling (`docs/core_architecture.md:31`, `docs/core_architecture.md:80`). Flesh out real execution (or wire through to a host scheduler) and add the promised cross-platform integration tests.

## TASK-004 – Set Up Logging & Tracing Infrastructure
- Although `LoggingConfig` accepts an optional `LoggerSink` (`core-runtime/src/logging.rs:65-139`), the initialization paths never wire it into the subscriber stack (`core-runtime/src/logging.rs:170-239`). Forward log events to the sink to meet the host logging integration goal stated in the architecture (`docs/core_architecture.md:230`).

## TASK-005 – Create Core Configuration System
- `CoreConfig::build` always errors unless callers manually inject `SecureStore` and `SettingsStore` (`core-runtime/src/config.rs:592-612`), which contradicts the acceptance note that the default desktop configuration works out of the box. Provide desktop defaults when the `desktop-shims` feature is enabled or update the task status.

## TASK-102 – Implement OAuth 2.0 Flow Manager
- `OAuthFlowManager::exchange_code` constructs a `reqwest::Client` directly (`core-auth/src/oauth.rs:325-330`), bypassing the host-provided `HttpClient` abstraction mandated for portability (`docs/core_architecture.md:25-33`). Pass an injected `HttpClient` (or reuse one from `CoreConfig`) so mobile/Web hosts can supply their own stacks.
