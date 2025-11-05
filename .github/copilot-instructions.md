# GitHub Copilot Instructions for Music Platform Core (Rust)

If you are struggling to implement a feature or complete a task, don't ever simplify the solution or code.
Say explicitly that you are unable to help if you cannot find a solution that meets all the requirements.
Or at least put TODO comments in the code where you are unsure.
Or even terminate the implementation if you cannot find a solution that meets all the requirements.
Don't workaround or simplify the requirements.
This is not a toy project, this is a production-grade cross-platform library that will be used in real world applications.
Don't use any simpler approaches or libraries that do not meet the performance, security, or cross-platform requirements.

## Context7 Usage

Always use context7 when I need code generation, setup or configuration steps, or
library/API documentation. This means you should automatically use the Context7 MCP
tools to resolve library id and get library docs without me having to explicitly ask.

## Update Task List status and high level architecture

Always update the task list status in the markdown file docs\ai_task_list.md when you complete a task.
Always follow the high level architecture and design principles outlined in docs/core_architecture.md
when generating code or implementing features.
