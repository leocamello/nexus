# Development Learnings

Lessons learned from implementing features in Nexus. Updated after each major feature.

---

## After F02, F03, F04 (Backend Registry, Health Checker, CLI & Configuration)

**Date**: 2026-02-03

### Workflow Learnings

#### 1. Spec-Kit Workflow is Effective
The spec → plan → tasks → implement → walkthrough flow works well:
- **spec.md**: Captures requirements, acceptance criteria, data structures
- **plan.md**: Technical design, phases, test strategy
- **tasks.md**: Detailed TDD tasks with tests-first approach
- **walkthrough.md**: Junior-friendly code explanation

**Recommendation**: Always create all four documents for features with 5+ tasks.

#### 2. GitHub Issues Should Be Created Early
Creating GitHub issues from tasks.md before implementation helps:
- Track progress visibly
- Reference issues in commits
- Close issues automatically with PR merge

**Checklist for creating issues**:
- [ ] Use feature prefix: `[Feature Name] T01: Task Title`
- [ ] Include labels: `P0`, `enhancement`, `<feature-label>`
- [ ] Reference dependencies: `Depends on #X`
- [ ] Link to spec files

#### 3. PR Labels Must Be Added at Creation Time
PR #44 was merged without labels - had to add them retroactively.

**PR creation checklist**:
- [ ] Add `enhancement` label
- [ ] Add feature-specific label (e.g., `cli-config`)
- [ ] Add priority label (`P0` for MVP)
- [ ] Link related issues with "Closes #X, #Y, #Z"

#### 4. Temporary Files Should Be Avoided
`IMPLEMENTATION_SUMMARY.md` was created during partial implementation but became outdated. The walkthrough.md serves as the permanent documentation.

**Recommendation**: Don't create temporary progress files in specs/. Use the session workspace instead.

### Technical Learnings

#### 1. Configuration Precedence Pattern
Layered configuration works well: `CLI > Env > File > Defaults`

```rust
// Load file first
let mut config = NexusConfig::load(path)?;
// Apply env overrides
config = config.with_env_overrides();
// Apply CLI overrides (in serve command)
if let Some(port) = args.port {
    config.server.port = port;
}
```

#### 2. View Models for CLI Output
Separating internal types from display types prevents coupling:
- `Backend` (internal, complex, atomics)
- `BackendView` (display, simple, serializable)

#### 3. Graceful Shutdown Pattern
Using `CancellationToken` from `tokio_util` provides clean shutdown:
```rust
let cancel_token = CancellationToken::new();
// Pass to background tasks
let handle = health_checker.start(cancel_token.clone());
// Wait for signal
shutdown_signal(cancel_token).await;
// Cleanup
handle.await?;
```

#### 4. Test Organization
- Unit tests in same file (`mod tests` at bottom)
- Integration tests in `tests/` directory
- Property tests for complex logic (router scoring)
- Doc tests for public API examples

### Code Review Findings (Non-Blocking)

1. **Signal handler panics**: Using `.expect()` on signal installation matches tokio patterns but could be improved with proper error handling.

2. **Placeholder fields**: `api_key_env` in BackendConfig is defined but unused - documented as placeholder for F05 (Authentication).

### Metrics

| Feature | Tasks | Tests | Lines Added | Time |
|---------|-------|-------|-------------|------|
| F02 Backend Registry | 8 | 58 | ~1200 | ~4h |
| F03 Health Checker | 11 | 46 | ~1100 | ~4h |
| F04 CLI & Config | 17 | 69 | ~2200 | ~6h |

### Process Improvements for Next Feature

1. **Add labels when creating PR** (not after merge)
2. **Run `cargo clippy --all-targets -D warnings`** before committing
3. **Include walkthrough.md in implementation** (not as separate step)
4. **Use `speckit.analyze` before creating GitHub issues** (catches inconsistencies early)

---

## Template for Future Entries

```markdown
## After F0X (Feature Name)

**Date**: YYYY-MM-DD

### Workflow Learnings
- What worked well
- What should change

### Technical Learnings
- Patterns discovered
- Code examples

### Code Review Findings
- Issues found in review
- How they were addressed

### Metrics
| Metric | Value |
|--------|-------|
| Tasks | X |
| Tests | Y |
| Lines | Z |

### Process Improvements
1. ...
```
