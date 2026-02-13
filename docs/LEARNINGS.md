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

## After F05 (mDNS Discovery)

**Date**: 2026-02-08

### Workflow Learnings

#### 1. Three-Checklist System Established
After completing F05, we applied verification checklists retroactively to all features and discovered the need for a formal requirements validation phase. The new workflow uses three checklists:

| Checklist | When | Items | Purpose |
|-----------|------|-------|---------|
| `requirements-validation.md` | BEFORE implementation | 65 | Validate spec is ready |
| `tasks.md` | DURING implementation | varies | Track acceptance criteria |
| `implementation-verification.md` | AFTER implementation | 210 | Verify implementation |

**Key Insight**: The requirements-validation checklist (condensed from the 208-item requirements-quality.md) serves as a quality gate before creating GitHub issues or starting implementation.

#### 2. Verification Baselines Established
All 5 implemented features now have verification checklists:
- F01 (API Gateway): 170 verified, 40 N/A
- F02 (Backend Registry): 120 verified, 90 N/A
- F03 (Health Checker): 147 verified, 63 N/A
- F04 (CLI & Config): 148 verified, 62 N/A
- F05 (mDNS Discovery): 138 verified, 72 N/A

**Total**: 723 items verified, 327 N/A, 0 blocking issues.

### Technical Learnings

#### 1. mDNS Service Type Normalization
Service types must end with a trailing dot for the mdns-sd crate. Instead of requiring users to know this, we normalize automatically:

```rust
let normalized = if service_type.ends_with('.') {
    service_type.clone()
} else {
    format!("{}.", service_type)
};
```

#### 2. Single-Machine mDNS Testing
Testing mDNS on a single machine requires OS-specific tools:
- **Linux**: `avahi-publish -s` to announce services
- **macOS**: `dns-sd -R` to register services

### Metrics

| Feature | Tasks | Tests | Issues Closed |
|---------|-------|-------|---------------|
| F05 mDNS Discovery | 12 | 29 | 12 (#59-#70) |

### Process Improvements for Next Feature (F06)

1. **Copy requirements-validation.md** to feature folder BEFORE creating issues
2. **Complete all 65 validation items** before `speckit.taskstoissues`
3. **Complete verification.md** before creating PR
4. **Follow 10-phase workflow** documented in `docs/SPEC_KIT_PROMPTS.md`

---

## After F06 (Intelligent Router)

**Date**: 2026-02-08

### Workflow Learnings

#### 1. Complex Features Benefit from Multi-Strategy Design
F06 implemented 4 routing strategies (smart, round-robin, priority, random) behind a single `select_backend()` entry point. Using enum dispatch instead of trait objects kept the routing path allocation-free and under 1ms.

#### 2. Property-Based Testing Validates Scoring Logic
The `score()` function in the router has multiple weighted inputs (priority, load, latency). Property-based testing with `proptest` proved more effective than hand-crafted test cases for validating edge cases in the scoring formula.

### Technical Learnings

#### 1. Lock-Free Round-Robin with AtomicU64
Thread-safe round-robin without mutexes:
```rust
let index = self.counter.fetch_add(1, Ordering::Relaxed);
let backend = candidates[index % candidates.len()];
```
`Ordering::Relaxed` is sufficient because we don't need happens-before guarantees — occasional duplicate selection is acceptable.

#### 2. Capability-First Filtering
The router filters by capabilities (vision, tools, context length) *before* scoring by load/latency. This prevents a fast-but-incapable backend from winning the selection.

#### 3. No I/O in the Hot Path
The router reads directly from the Registry's `DashMap` — no network calls, no disk I/O. This is what makes sub-1ms routing possible.

### Metrics

| Metric | Value |
|--------|-------|
| Tasks | 15 |
| Tests | 48+ |
| PR | #87 |

---

## After F07 (Model Aliases)

**Date**: 2026-02-08

### Workflow Learnings

#### 1. Small Features Can Still Have Subtle Complexity
F07 was only 6 tasks, but alias chaining and circular detection required careful design. The decision to validate at config load time (fail-fast) rather than request time was critical for maintaining routing performance.

#### 2. Same-Day Feature Turnaround
F07 went from spec to merged PR in a single session. The spec-kit workflow scales down well for small features — the overhead of spec.md + tasks.md + walkthrough.md was minimal and the documentation payoff was worth it.

### Technical Learnings

#### 1. Iterative Alias Resolution (Not Recursive)
Chose iterative loop over recursion to prevent stack overflow on deep chains:
```rust
fn resolve_alias(&self, model: &str) -> String {
    let mut current = model.to_string();
    for _ in 0..MAX_DEPTH {
        match self.aliases.get(&current) {
            Some(target) => current = target.clone(),
            None => break,
        }
    }
    current
}
```

#### 2. Cycle Detection with HashSet
Circular aliases (A→B→C→A) are detected at config load using a visited set. This is O(n) per chain and runs once at startup, not per-request.

### Metrics

| Metric | Value |
|--------|-------|
| Tasks | 6 |
| Tests | 17 |
| PR | #94 |
| Issues Closed | via PR |

---

## After F08 (Fallback Chains)

**Date**: 2026-02-08

### Workflow Learnings

#### 1. Feature Extensions Should Reference Parent Specs
F08 extended F06's fallback logic (already partially implemented) by adding the `X-Nexus-Fallback-Model` response header. The tasks.md correctly built on F06's T10-T12, adding only 4 new tasks (T07-T10). This avoids spec duplication.

#### 2. Headers as Observability Signals
Adding `X-Nexus-Fallback-Model` was a small change with high diagnostic value. When debugging, seeing which model actually served a request (vs. what was requested) is immediately actionable.

### Technical Learnings

#### 1. RoutingResult as Return Type
Changing `select_backend()` to return a `RoutingResult` struct instead of `Arc<Backend>` enabled carrying metadata (fallback_used, actual_model) without breaking the existing API:
```rust
pub struct RoutingResult {
    pub backend: Arc<Backend>,
    pub actual_model: String,
    pub fallback_used: bool,
}
```

#### 2. Conditional Header Injection
The `X-Nexus-Fallback-Model` header is only added when a fallback was actually used. This keeps responses clean for the common case (primary model available).

### Metrics

| Metric | Value |
|--------|-------|
| Tasks | 4 (new) + 6 (from F06) |
| Tests | ~10 |
| PR | #99 |
| Issues Closed | #95, #96, #97, #98 |

---

## After F09 (Request Metrics)

**Date**: 2026-02-13

### Workflow Learnings

#### 1. Phase 7 "Polish" Items Should Not Be Deferred
F09 initially deferred 23 Phase 7 tasks (benchmarks, property tests, integration tests, documentation). This created a second PR (#108) to complete them. **Lesson**: Polish tasks (benchmarks, property tests, README updates) should be part of the initial implementation, not deferred. They catch real issues — the benchmark fix revealed that no Criterion benchmarks in the entire project were actually executing.

#### 2. Requirements-Validation.md Should Be Created Early
F09's `requirements-validation.md` was created retroactively after implementation. While the validation confirmed the spec was solid, creating it *before* implementation (as the process prescribes) would have caught potential spec gaps earlier.

#### 3. Two-PR Pattern Is Sometimes Necessary
F09 used two PRs: #107 (core implementation) and #108 (deferred items). While not ideal, this was pragmatic — it allowed the core feature to land and be validated while polish work continued. For future features, aim for a single PR.

### Technical Learnings

#### 1. Global Metrics Recorder Isolation (CRITICAL)
The `metrics` crate (v0.24) uses a **global recorder** set via `install_recorder()` — can only be called once per process. In `cargo test`, all tests share one process:

```
AppState::new() #1 → install_recorder() → SUCCESS (owns global)
AppState::new() #2 → install_recorder() → FAILS (gets detached handle)
```

`counter!()`/`gauge!()` macros write to the **global** recorder, but `render_metrics()` reads from `self.prometheus_handle`. For non-first AppStates, the handle is **detached** — it sees nothing.

**Solution**: Integration tests verify behavior through HTTP status codes, JSON schemas, and Registry atomics (via `/v1/stats`) instead of parsing Prometheus text output.

**Takeaway**: Any crate using global state (metrics, tracing, etc.) requires careful test isolation design.

#### 2. Criterion Benchmarks Require Cargo.toml Entries
Criterion uses `criterion_main!` which defines its own `main()`. Without `[[bench]] harness = false` in `Cargo.toml`, `cargo bench` runs them as the default test harness (shows "running 0 tests"):

```toml
[[bench]]
name = "metrics"
harness = false
```

**All 3 benchmark files** (cli_startup, config_parsing, metrics) were silently not executing before this fix.

**Docker caveat**: `[[bench]]` entries make Cargo require bench files to exist even for `cargo build`. Since `.dockerignore` excludes `benches/`, the Dockerfile must create stub bench files (`echo "fn main() {}" > benches/X.rs`) in the dependency-cache layer.

#### 3. Label Sanitization with DashMap Caching
Prometheus labels must be alphanumeric + underscore. Model names like `llama3:70b` or `my-backend` need sanitizing. A `DashMap` cache prevents re-sanitizing the same label on every request:

```rust
fn sanitize_label(input: &str) -> String {
    if let Some(cached) = self.label_cache.get(input) {
        return cached.clone();
    }
    let sanitized = /* regex replace */;
    self.label_cache.insert(input.to_string(), sanitized.clone());
    sanitized
}
```

Benchmark: ~50ns cached vs ~359ns uncached.

#### 4. Two Endpoints, Two Audiences
- **`GET /metrics`**: Prometheus text format — for Grafana, alerting, time-series DBs
- **`GET /v1/stats`**: JSON format — for developers, dashboards, debugging

The JSON endpoint reads from Registry atomics directly, avoiding the global recorder issue entirely.

### Metrics

| Metric | Value |
|--------|-------|
| Tasks | 78 (7 phases) |
| Unit Tests | 9 |
| Integration Tests | 22 |
| Property Tests | 2 |
| Benchmarks | 4 |
| PRs | #107, #108 |
| Issues Closed | #101-#106 |

### Performance Benchmarks

| Benchmark | Result | Budget |
|-----------|--------|--------|
| metric_recording_overhead | ~188ns | < 100µs |
| metrics_endpoint_render | ~3.8µs | N/A |
| stats_endpoint_compute | ~5.9µs | N/A |
| label_sanitize_cached | ~50ns | N/A |
| label_sanitize_uncached | ~359ns | N/A |

---

## v0.1 Full Retrospective

**Date**: 2026-02-13
**Scope**: F01 (Backend Registry) through F09 (Request Metrics)

### What Went Well

#### 1. Spec-Kit Workflow Proved Its Value
The 4-phase workflow (Spec → Implement → Verify → Merge) created consistent, high-quality documentation across all 9 features. Every feature has: `spec.md`, `plan.md`, `tasks.md`, `walkthrough.md`, and `verification.md`. New developers can onboard by reading walkthroughs.

#### 2. Constitution-Driven Architecture
The 10 constitutional principles (especially "Control Plane, Not Data Plane" and "Stateless") prevented architectural drift. When making decisions, asking "does this violate the constitution?" provided clear answers.

#### 3. Test-First Approach
TDD caught issues early. The project has **389 tests** (284 unit + 105 integration/doc) with zero failures. Property testing with `proptest` validated complex scoring logic that hand-crafted tests would have missed.

#### 4. Single-Binary Simplicity
Every dependency choice preserved the single-binary goal. No external databases, no separate config services, no sidecar processes. `cargo build` produces one executable.

### What Should Improve

#### 1. Process Discipline Varied Across Features
Early features (F02-F04) didn't have `requirements-validation.md`. F09 deferred Phase 7 items. The three-checklist system was established mid-project (after F05) and should be mandatory from day one for v0.2.

**Action**: Follow the 4-phase process strictly for every v0.2 feature. No deferred items.

#### 2. PR Size Should Stay Small
F09 required two PRs because the initial one deferred 23 tasks. Smaller, complete PRs are easier to review and less likely to introduce the kind of issues we saw (benchmark harness misconfiguration went unnoticed).

**Action**: If a feature has 7+ phases, consider splitting into multiple features or shipping each phase as its own PR.

#### 3. Integration Test Isolation Needs Attention
The global metrics recorder issue (F09) and potential future crates with global state (tracing subscribers, etc.) need a testing strategy. Each integration test file should document its isolation assumptions.

**Action**: Add a comment at the top of integration test files explaining global state constraints.

#### 4. Manual Testing Guide Is Outdated
`docs/MANUAL_TESTING_GUIDE.md` hasn't been updated since F05. It should cover the new `/metrics` and `/v1/stats` endpoints.

**Action**: Update before starting F10.

### Process Maturity Timeline

| Phase | Features | Process Level |
|-------|----------|---------------|
| Early v0.1 | F02-F04 | Basic: spec + tasks + walkthrough |
| Mid v0.1 | F05-F06 | Improved: + verification + requirements-validation |
| Late v0.1 | F07-F08 | Mature: Full 4-phase + issues + PR labels |
| v0.2 Start | F09 | Hardened: + benchmarks + property tests + 2-PR lesson |

### Cumulative Project Metrics

| Metric | Value |
|--------|-------|
| Features Shipped | 9 |
| PRs Merged | 9 |
| Total Tests | 389 |
| GitHub Issues Closed | 50+ |
| Spec-Kit Artifacts | 45+ files |
| Constitutional Principles | 10 |

### Recommendations for v0.2

1. **Single PR per feature** — no deferred items
2. **Create `requirements-validation.md` BEFORE writing code** — it's a quality gate
3. **Run `speckit.analyze` twice** — once after spec, once before PR
4. **Include benchmarks in initial implementation** — don't defer performance validation
5. **Update MANUAL_TESTING_GUIDE.md with each feature** — not retroactively
6. **Test isolation documentation** — comment global state assumptions in test files

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
