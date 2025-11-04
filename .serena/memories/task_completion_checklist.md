# Task Completion Checklist

When completing any task or feature implementation, follow this checklist:

## Code Quality

### 1. Formatting
```bash
cargo fmt --all
```
- [ ] Code is properly formatted
- [ ] No formatting warnings

### 2. Linting
```bash
cargo clippy --all-targets --all-features -- -D warnings
```
- [ ] No clippy warnings (treat warnings as errors)
- [ ] All clippy suggestions addressed or explicitly allowed with justification

### 3. Compilation
```bash
cargo build --workspace --all-features
```
- [ ] All crates compile without errors
- [ ] All features compile without errors
- [ ] No unused dependencies

## Testing

### 4. Unit Tests
```bash
cargo test --workspace
```
- [ ] All existing tests pass
- [ ] New functionality has unit tests
- [ ] Both success and error paths are tested
- [ ] Edge cases are covered

### 5. Integration Tests
```bash
cargo test --test '*'
```
- [ ] Integration tests pass (if applicable)
- [ ] New features have integration tests (if applicable)

### 6. Test Coverage
- [ ] Critical paths have >80% coverage
- [ ] Error handling is tested
- [ ] Mock implementations used appropriately

## Documentation

### 7. Code Documentation
```bash
cargo doc --no-deps --all-features
```
- [ ] All public functions have doc comments
- [ ] Doc comments include:
  - Description of what the function does
  - `# Arguments` section for parameters
  - `# Returns` section for return values
  - `# Errors` section for error conditions
  - `# Examples` section with usage examples
- [ ] Module-level documentation is present
- [ ] Documentation builds without warnings

### 8. Examples
- [ ] Complex features have runnable examples
- [ ] Examples are placed in `examples/` directory
- [ ] Examples compile and run successfully

## Error Handling

### 9. Error Types
- [ ] Errors use `thiserror` for structured error types
- [ ] Error messages are actionable and descriptive
- [ ] Error types implement `Debug` and `Display`
- [ ] Errors include context where helpful

### 10. Error Propagation
- [ ] Errors are propagated with `?` operator
- [ ] No `.unwrap()` or `.expect()` in production code (except in tests or with clear justification)
- [ ] Panic messages include actionable remediation steps

## Logging & Tracing

### 11. Instrumentation
- [ ] Public async functions use `#[instrument]` macro
- [ ] Important operations have structured logging
- [ ] Log levels are appropriate:
  - `error!` - Failures requiring attention
  - `warn!` - Degraded functionality
  - `info!` - Important state changes
  - `debug!` - Detailed debugging info
  - `trace!` - Very detailed tracing

### 12. PII Protection
- [ ] No tokens logged
- [ ] Email addresses redacted
- [ ] File paths stripped to basename
- [ ] No sensitive data in logs

## Security

### 13. Credential Handling
- [ ] Tokens stored via `SecureStore` trait
- [ ] No hardcoded secrets
- [ ] Secure erasure on cleanup

### 14. Input Validation
- [ ] All user inputs validated
- [ ] Bounds and limits checked
- [ ] Invalid input returns descriptive errors

## Performance

### 15. Resource Management
- [ ] No unbounded collections
- [ ] Memory usage is bounded
- [ ] Database connections pooled
- [ ] Large result sets streamed or paginated

### 16. Async Best Practices
- [ ] I/O operations are async
- [ ] Blocking operations on `spawn_blocking`
- [ ] Cancellation supported for long operations
- [ ] Timeouts used for network operations

## Integration

### 17. API Consistency
- [ ] Public API follows project conventions
- [ ] Naming is consistent with existing code
- [ ] Types use newtype pattern where appropriate
- [ ] Traits used for abstraction

### 18. Event Emission
- [ ] State changes emit appropriate events
- [ ] Events are properly typed
- [ ] Event documentation is clear

## Platform Considerations

### 19. Bridge Abstractions
- [ ] Platform-specific code uses bridge traits
- [ ] No direct platform dependencies in core modules
- [ ] Fail-fast when required bridges missing
- [ ] Graceful degradation for optional features

### 20. Feature Flags
- [ ] Optional functionality gated by features
- [ ] Features documented in `Cargo.toml`
- [ ] Disabled features don't break compilation

## Final Checks

### 21. Version Control
```bash
git status
git diff
```
- [ ] Changes reviewed
- [ ] Commit message is descriptive
- [ ] No unintended changes included

### 22. Dependencies
- [ ] New dependencies justified
- [ ] Dependency versions pinned in workspace
- [ ] No duplicate dependencies with different versions

### 23. Before Push
```bash
cargo test --workspace --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
cargo doc --no-deps --all-features
```
- [ ] All commands pass successfully
- [ ] No warnings or errors
- [ ] Ready for review

## Summary Checklist

Quick reference for task completion:
- [ ] ✅ Code formatted
- [ ] ✅ Clippy passed with no warnings
- [ ] ✅ All tests passing
- [ ] ✅ Documentation complete
- [ ] ✅ Errors are actionable
- [ ] ✅ Logging instrumented
- [ ] ✅ No PII in logs
- [ ] ✅ Security considered
- [ ] ✅ Performance acceptable
- [ ] ✅ Platform abstractions used
- [ ] ✅ Ready to commit
