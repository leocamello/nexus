# Nexus Quality Assurance Checklists

This document explains the dual checklist system for ensuring feature quality in the Nexus project.

## Overview

Nexus uses **two complementary checklists** to ensure both specification quality and implementation correctness:

1. **Requirements Quality Checklist** - Validates spec quality BEFORE implementation
2. **Implementation Verification Checklist** - Validates implementation correctness AFTER development

## The Two-Checklist System

### 📋 Requirements Quality Checklist

**File**: `.specify/checklists/requirements-quality.md`  
**Purpose**: Unit tests for requirements writing - validates specification quality  
**When**: During spec review phase, BEFORE implementation planning  
**Who**: Feature authors, spec reviewers, architects

**What it validates**:
- ✅ Requirements are complete, clear, consistent, and measurable
- ✅ Spec follows established patterns and structure
- ✅ Constitutional principles are addressed
- ✅ Technical constraints are respected
- ✅ Testing approach is documented
- ✅ Dependencies and edge cases are identified

**What it does NOT validate**:
- ❌ Whether code works correctly
- ❌ Whether implementation matches spec
- ❌ Whether tests pass

**Think of it as**: Linting and unit testing your requirements document

---

### ✅ Implementation Verification Checklist

**File**: `.specify/templates/implementation-verification.md`  
**Purpose**: Verify implementation correctness and completeness  
**When**: During and after implementation, BEFORE PR merge  
**Who**: Feature implementers, code reviewers, QA

**What it validates**:
- ✅ All acceptance criteria are met
- ✅ Tests pass and provide adequate coverage
- ✅ Code quality standards are met (clippy, fmt, docs)
- ✅ Constitutional principles are upheld in code
- ✅ Manual testing confirms correct behavior
- ✅ Integration points work correctly

**What it does NOT validate**:
- ❌ Whether requirements are well-written
- ❌ Whether spec is complete (use requirements checklist for that)

**Think of it as**: Acceptance testing your implementation

---

## When to Use Each Checklist

### Feature Development Workflow

```
1. Write Spec
   ↓
2. ✅ Run Requirements Quality Checklist
   ├─ If issues found → Improve spec, repeat step 2
   └─ If spec passes → Proceed to step 3
   ↓
3. Review & Approve Spec
   ↓
4. Plan Implementation (tasks.md)
   ↓
5. Write Tests (TDD Red phase)
   ↓
6. Implement Feature (TDD Green phase)
   ↓
7. ✅ Run Implementation Verification Checklist
   ├─ If issues found → Fix implementation, repeat step 7
   └─ If implementation passes → Proceed to step 8
   ↓
8. Create PR & Merge
```

### Quick Reference

| Phase | Checklist | Purpose |
|-------|-----------|---------|
| **Spec Writing** | Requirements Quality | Ensure spec is clear and complete |
| **Spec Review** | Requirements Quality | Validate spec before approval |
| **Implementation** | Implementation Verification | Track progress and ensure nothing is missed |
| **Pre-PR** | Implementation Verification | Final check before creating PR |
| **Code Review** | Implementation Verification | Reviewer validates completeness |

---

## Key Differences

| Aspect | Requirements Quality | Implementation Verification |
|--------|----------------------|----------------------------|
| **Focus** | Specification quality | Implementation correctness |
| **Timing** | Before coding | During/after coding |
| **Questions** | "Are requirements clear?" | "Does code meet requirements?" |
| **Example** | "Are latency targets quantified?" | "Does routing meet <1ms target?" |
| **Pass Criteria** | Spec is ready for implementation | Feature is ready for merge |

---

## Checklist Locations

### Requirements Quality Checklist

- **Generated checklists**: `.specify/checklists/*.md`
- **Generation command**: `/speckit.checklist` (via GitHub Copilot)
- **Frequency**: One per feature (e.g., `requirements-quality.md` for project-wide, or `ux.md`, `security.md` for specific domains)

### Implementation Verification Checklist

- **Template**: `.specify/templates/implementation-verification.md`
- **Per-feature copies**: `specs/XXX-feature-name/verification.md` (copy template and customize)
- **Frequency**: One per feature implementation

---

## How to Use the Requirements Quality Checklist

### 1. During Spec Writing

As you write the spec, reference the checklist to ensure you include all necessary sections:

```bash
# View checklist while writing spec
cat .specify/checklists/requirements-quality.md
```

**Pro tip**: Keep the checklist open in a split window while writing your spec.

### 2. Before Spec Review

Run through the checklist and check off items:

- [ ] CHK001: Is the Simplicity Gate explicitly checked in the plan?
- [x] CHK002: Is the Anti-Abstraction Gate explicitly checked?
- ...

Mark items as:
- `[x]` - Requirement met
- `[ ]` - Requirement not met (needs work)
- `[N/A]` - Not applicable to this feature

### 3. During Spec Review

Reviewer uses the checklist to systematically validate spec quality:

```markdown
## Review Findings (using requirements-quality.md)

- ❌ CHK015: Performance targets not quantified (see NFR section)
- ❌ CHK045: Edge cases missing for concurrent access
- ✅ CHK089: User stories have proper Given/When/Then format
```

---

## How to Use the Implementation Verification Checklist

### 1. Copy Template for Your Feature

```bash
# Copy template to your feature directory
cp .specify/templates/implementation-verification.md \
   specs/005-your-feature/verification.md
```

### 2. Customize for Your Feature

Remove N/A sections and add feature-specific items:

```markdown
### Feature-Specific Verification

- [ ] VER-300: mDNS service discovery works for _ollama._tcp.local
- [ ] VER-301: Grace period prevents backend flapping
- [ ] VER-302: Background discovery task can be cancelled cleanly
```

### 3. Track Progress During Implementation

Check off items as you complete them:

```markdown
- [x] VER-009: Tests written before implementation (git log shows)
- [x] VER-014: All public functions have unit tests
- [ ] VER-040: Routing decision < 1ms (TODO: add benchmark)
```

### 4. Final Check Before PR

Run through entire checklist to ensure nothing is missed:

```bash
# Count unchecked items
grep -c "\- \[ \]" specs/005-your-feature/verification.md

# Goal: 0 unchecked items (or all justified as N/A)
```

---

## Checklist Evolution

Both checklists should evolve as the project matures:

### Adding Items

When you discover a quality issue not caught by existing checklists:

1. **Document the issue** - What went wrong? What should have been checked?
2. **Add checklist item** - Add specific, actionable item to prevent recurrence
3. **Update template** - Ensure future features benefit from the learning

### Removing Items

When items become obsolete or redundant:

1. **Identify outdated items** - Does this still apply?
2. **Justify removal** - Why is this no longer needed?
3. **Archive reasoning** - Document in checklist version history

### Refinement

After completing 5 feature specs:

1. **Review effectiveness** - Which items caught real issues?
2. **Consolidate duplicates** - Merge similar items
3. **Adjust granularity** - Too detailed? Too vague?
4. **Update examples** - Ensure examples reflect current patterns

---

## Integration with speckit Agents

The checklists integrate with speckit agents:

### speckit.checklist Agent

Generates custom requirements quality checklists:

```bash
/speckit.checklist

# Example: Generate a UX-focused checklist
"Create a UX requirements quality checklist based on the landing page feature spec"

# Result: .specify/checklists/ux.md
```

### speckit.analyze Agent

Analyzes specs and tasks for consistency:

```bash
/speckit.analyze

# Checks:
# - Consistency between spec.md, plan.md, tasks.md
# - Completeness of acceptance criteria
# - Traceability of requirements
```

**Pro tip**: Run `speckit.analyze` before using the requirements quality checklist to catch basic issues first.

---

## Constitutional Alignment

Both checklists enforce Nexus Constitution principles:

### Requirements Quality Checklist Enforces:

- **Specification clarity**: Are principles like "Zero Configuration" explicitly addressed in requirements?
- **Design constraints**: Are Rust, Tokio, Axum, reqwest specified?
- **Performance targets**: Are Constitution latency budgets referenced in NFRs?
- **Test-First approach**: Is TDD workflow documented in the spec?

### Implementation Verification Checklist Enforces:

- **Constitution Gates**: Simplicity, Anti-Abstraction, Integration-First, Performance
- **Code standards**: Clippy, fmt, no println!, tracing, thiserror
- **TDD compliance**: Tests written first, RED → GREEN → Refactor
- **Performance budgets**: < 1ms routing, < 5ms overhead, < 50MB memory

---

## Examples from Nexus Features

### Example 1: Backend Registry (001)

**Requirements Quality Checklist Usage**:
- ✅ Validated that all atomic operations (pending_requests) were specified
- ✅ Confirmed that concurrency requirements were quantified (10,000+ concurrent reads)
- ✅ Verified that edge cases like "decrement below 0" were documented

**Implementation Verification Checklist Usage**:
- ✅ Verified DashMap used for concurrent access (Anti-Abstraction Gate)
- ✅ Confirmed stress tests pass with 10,000+ concurrent operations
- ✅ Validated that saturating_sub used for pending_requests decrement

### Example 2: Health Checker (002)

**Requirements Quality Checklist Usage**:
- ✅ Confirmed health check interval and timeout were quantified
- ✅ Verified that failure/recovery thresholds were specified
- ✅ Ensured that cancellation (graceful shutdown) was addressed

**Implementation Verification Checklist Usage**:
- ✅ Verified CancellationToken used for clean shutdown
- ✅ Confirmed background task can be stopped gracefully
- ✅ Validated that health transitions work (3 failures → Unhealthy)

### Example 3: CLI Configuration (003)

**Requirements Quality Checklist Usage**:
- ✅ Verified that configuration precedence was specified (CLI > Env > Config > Defaults)
- ✅ Confirmed that all CLI commands and flags were documented
- ✅ Ensured error messages and exit codes were specified

**Implementation Verification Checklist Usage**:
- ✅ Verified clap derive pattern used (Anti-Abstraction Gate)
- ✅ Confirmed precedence logic works (tests for CLI overriding config)
- ✅ Validated that `nexus serve` works with zero config (Zero Configuration principle)

---

## Best Practices

### For Spec Authors

1. **Reference checklist while writing** - Don't wait until review
2. **Self-review first** - Run through checklist before submitting for review
3. **Justify omissions** - If a checklist item doesn't apply, explain why
4. **Update checklist** - Add items if you find gaps during spec writing

### For Spec Reviewers

1. **Use checklist systematically** - Don't skip sections
2. **Be specific in feedback** - Reference checklist item numbers (e.g., "CHK045 not met")
3. **Prioritize critical items** - Not all items are equally important
4. **Suggest improvements** - Don't just reject, help improve the spec

### For Implementers

1. **Copy template early** - Create verification checklist at start of implementation
2. **Track progress** - Check off items as you complete them
3. **Use as guide** - Let checklist guide what to implement
4. **Update continuously** - Don't wait until end to run through checklist

### For Code Reviewers

1. **Verify checklist completion** - Ensure implementer ran through checklist
2. **Spot check critical items** - Validate key items (tests, performance, constitutional compliance)
3. **Trust but verify** - Checked items should be accurate, but sample verification
4. **Approve only when complete** - All items checked or justified as N/A

---

## FAQ

### Q: Do I need to check every item for every feature?

**A**: Most items apply to most features, but use judgment. Mark items as `[N/A]` if truly not applicable, and briefly explain why.

### Q: What if a checklist item conflicts with my feature's unique needs?

**A**: Document the conflict and justify why you're deviating. Update the checklist with a note about exceptions.

### Q: How detailed should my justifications be?

**A**: Brief but clear. E.g., "N/A - This feature has no API endpoints" or "Deferred to F06 - Router will implement this".

### Q: Can I skip sections that don't apply?

**A**: Yes, but be explicit. Comment out N/A sections with a note: `<!-- N/A: No CLI commands in this feature -->`.

### Q: What if I find an issue after implementation is "complete"?

**A**: Fix it, add a checklist item for future features, and document the lesson learned.

### Q: How do checklists relate to acceptance criteria in tasks.md?

**A**: Acceptance criteria are task-level details. Verification checklist aggregates and validates all acceptance criteria are met.

---

## Continuous Improvement

These checklists are living documents. After each feature:

1. **Retrospective**: What did the checklists catch? What did they miss?
2. **Update**: Add items for missed issues, remove outdated items
3. **Refine**: Improve clarity, examples, and organization
4. **Share learnings**: Update this README with new examples and patterns

---

## Summary

- **Two checklists, two purposes**: Requirements quality (BEFORE) and implementation verification (AFTER)
- **Requirements Quality**: Tests whether the spec is well-written and complete
- **Implementation Verification**: Tests whether the code meets the spec
- **Use systematically**: Don't skip checklists; they catch issues early
- **Evolve continuously**: Update checklists as you learn what works

**The goal**: Catch issues early, maintain high quality, and ensure Constitutional compliance throughout the development lifecycle.

---

**Version**: 1.0.0  
**Last Updated**: 2026-02-03  
**Next Review**: After 5 feature specs completed
