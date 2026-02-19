# GitHub Issue Patterns

Authoritative reference for creating GitHub issues in the Nexus project. All future issues MUST follow these patterns.

## Title Format

```
[TAG] type: Description (Scope)
```

### Components

| Component | Format | Examples |
|-----------|--------|----------|
| **TAG** | Feature ID(s) in brackets | `[F19]`, `[F20]`, `[F19/F20]` |
| **type** | Conventional commit type | `feat:`, `chore:`, `test:`, `docs:` |
| **Description** | Concise, lowercase-start | `Manual model placement control` |
| **Scope** | Phase, user story, or task range in parens | `(US1)`, `(Phase 1-2)`, `(T001-T006)` |

### Title Examples by Issue Type

```
# Foundation/setup issues (shared across features → use all feature IDs)
[F19/F20] feat: Setup — documentation and contracts (T001-T006)
[F19/F20] feat: Foundational types and infrastructure (T007-T017)

# User story issues (single feature → use that feature ID)
[F20] feat: Manual model placement control — MVP (US1)
[F20] feat: Model migration across backends (US2)
[F19] feat: Fleet intelligence and pre-warming recommendations (US4)

# Polish/chore issues (shared → use all feature IDs)
[F19/F20] chore: Polish — validation, docs, and observability (T081-T092)
```

### Conventions

- MVP user story gets `— MVP` suffix in title
- NO emoji in titles (e.g., no 🎯)
- NO version prefix in title (e.g., no `feat(v0.5):`)
- Task ranges `(T001-T006)` used for phase/infrastructure issues
- User story labels `(US1)` used for story issues
- Lowercase after colon: `feat: Manual model...` not `feat: Manual Model...`
  - Exception: proper nouns and acronyms (e.g., `MVP`, `VRAM`, `Ollama`)

## Labels

Every issue MUST have exactly these label categories:

| Label | Required | Source |
|-------|----------|--------|
| `enhancement` | Always | Standard GitHub label |
| `v0.X` | Always | Version milestone (e.g., `v0.5`) |
| Feature ID(s) | Always | `F19`, `F20`, etc. |

### Label Rules

1. **No priority labels** (`P0`, `P1`, `P2`) — priority lives in the spec, not labels
2. **No custom category labels** (no `phase-1`, `US1`, `tests`, `implementation`, etc.)
3. **Shared/foundation issues** get ALL related feature labels (e.g., `F19, F20`)
4. **Single-feature issues** get only their feature label (e.g., `F20`)
5. Feature labels use format `FXX` with description (e.g., label name `F19`, description `F19: Pre-warming & Fleet Intelligence`)
6. Feature label color: `#0075ca` (blue) for new features

## Body Format

### Header Section (mandatory)

```markdown
## FXX Feature Name — Issue Title

**Feature**: FXX Feature Name
**Branch**: `NNN-feature-branch-name`
**Phase**: Phase description (e.g., "User Story 1 🎯 MVP", "Setup", "Polish")
**Spec**: `specs/NNN-feature-name/`
```

### Body Sections (by issue type)

#### Foundation/Setup Issues

```markdown
### Description
Brief description of what this phase delivers.

### Tasks
- [ ] **TXXX** Task description in file/path
- [ ] **TXXX** [P] Parallelizable task description

### Acceptance Criteria
- [ ] Criterion 1
- [ ] Criterion 2
```

#### User Story Issues

```markdown
### Goal
One-sentence goal statement.

### Tests (write first, verify they fail)
- [ ] **TXXX** [P] Test description in tests/path

### Implementation
- [ ] **TXXX** [P] Implementation description in src/path

### Acceptance Criteria
- [ ] Criterion 1
- [ ] Criterion 2

### Dependencies
- Requires #NNN (description) to be complete
```

#### Polish/Chore Issues

```markdown
### Category 1 (e.g., Documentation)
- [ ] **TXXX** Task description

### Category 2 (e.g., Performance Validation)
- [ ] **TXXX** Task description

### Acceptance Criteria
- [ ] Criterion 1

### Dependencies
- Requires #NNN, #NNN, #NNN to be complete
```

### Conventions

- Task IDs are **bold**: `**T001**`
- Parallelizable tasks marked with `[P]`
- Dependencies reference issue numbers with description: `#195 (Foundational types)`
- Task count summary at bottom: `**Tasks**: 20 | **Parallel**: 7`
- Acceptance criteria use checkboxes for tracking

## Issue Grouping Strategy

### How to Group Tasks into Issues

| Issue Type | Grouping Rule | Example |
|------------|---------------|---------|
| Setup/Foundation | All setup tasks in one issue | T001-T006 |
| Foundational | All blocking prerequisite tasks in one issue | T007-T017 |
| User Story | ALL tasks for one user story (tests + implementation) in one issue | T018-T037 |
| Polish | All polish/validation tasks in one issue | T081-T092 |

### Key Rules

1. **User stories are NOT split** into separate test and implementation issues
2. **Foundation phase** is always one issue, marked as blocking
3. **Polish phase** is always one issue at the end
4. Issue count per feature: typically 5-8 issues (1 setup + 1 foundation + N user stories + 1 polish)

## Feature Label Creation

When starting a new feature, create labels BEFORE creating issues:

```bash
gh label create "FXX" --description "FXX: Feature Description" --color "0075ca"
```

## Evolution History

| Version | Change | Issues Affected |
|---------|--------|----------------|
| v0.1 | Initial patterns (`[Backend Registry] T01:` style) | #1-#11 |
| v0.1 | Standardized to `[FXX] type:` format | #59-#98 |
| v0.2 | Added version labels, task ranges | #101-#124 |
| v0.3 | Added branch metadata, user story grouping | #127-#171 |
| v0.4 | Consolidated multi-feature foundation issues | #173-#177 |
| v0.5 | Dropped priority labels, documented patterns | #194-#200 |
