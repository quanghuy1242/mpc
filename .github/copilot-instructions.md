# GitHub Copilot Instructions for Music Platform Core (Rust)

If you are struggling to implement a feature or complete a task, don't ever simplify the solution or code.
Say explicitly that you are unable to help if you cannot find a solution that meets all the requirements.
Or at least put TODO comments in the code where you are unsure.
Or even terminate the implementation if you cannot find a solution that meets all the requirements.
Don't workaround or simplify the requirements.
This is not a toy project, this is a production-grade cross-platform library that will be used in real world applications.
Don't use any simpler approaches or libraries that do not meet the performance, security, or cross-platform requirements.

## Memory & Context

**CRITICAL**: Always check and refer to persistent memory before starting any task. Use the memory tool to access:

- `/memories/mpc_project_context.md` - High-level project overview, requirements, phases, threading models
- `/memories/mpc_modules_detailed.md` - Complete reference of all 8 core modules, their APIs, usage patterns, and integration examples

These memory files contain essential context about the project architecture, module structure, platform differences (Native vs WASM), threading models, and usage patterns. Reference them to ensure consistency and avoid forgetting critical architectural decisions.

When working on any module or feature:
1. Read the relevant memory file first
2. Follow the documented patterns and APIs
3. Update memory if you discover new important information

## Context7 Usage

Always use context7 when I need code generation, setup or configuration steps, or
library/API documentation. This means you should automatically use the Context7 MCP
tools to resolve library id and get library docs without me having to explicitly ask.

## Update Task List status and high level architecture

Always update the task list status in the markdown file docs\ai_task_list.md when you complete a task.
Always follow the high level architecture and design principles outlined in docs/core_architecture.md
when generating code or implementing features.

## Module Integration Patterns

Refer to `/memories/mpc_modules_detailed.md` for:
- Complete API reference for all 8 core modules
- Native vs WASM integration patterns
- Direct module usage (native) vs CoreService (WASM/FFI)
- Threading models and worker pool patterns
- Platform-specific bridge implementations

## Architecture Decisions

Refer to `/memories/mpc_project_context.md` for:
- Why CoreService is optional for native but required for WASM
- 3-bundle WASM architecture (main, worker, audio)
- Threading models (multi-threaded Tokio vs Web Workers)
- Cross-platform strategy (Arc vs Rc, Send+Sync vs !Send)
- Phase 6 implementation roadmap
