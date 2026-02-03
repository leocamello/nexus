# Nexus Checklist System - Quick Reference Card

## 📋 Two Checklists, Two Purposes

| Checklist | When | Purpose | File |
|-----------|------|---------|------|
| **Requirements Quality** | BEFORE implementation | Validate spec quality | `.specify/checklists/requirements-quality.md` |
| **Implementation Verification** | AFTER implementation | Validate implementation correctness | `.specify/templates/implementation-verification.md` |

---

## 🚀 Quick Start

### For Spec Authors (Using Requirements Quality Checklist)

```bash
# 1. Write your spec (reference checklist while writing)
vim specs/XXX-your-feature/spec.md

# 2. Open checklist in split window
code .specify/checklists/requirements-quality.md

# 3. Self-review against checklist
# Check off items as you verify them

# 4. Submit for team review
# Reviewer uses same checklist
```

**Key Questions**:
- ✅ Are requirements complete, clear, consistent?
- ✅ Are all Constitution principles addressed?
- ✅ Are NFRs quantified with specific metrics?
- ✅ Are edge cases documented?
- ✅ Is testing approach defined?

---

### For Implementers (Using Implementation Verification Checklist)

```bash
# 1. Copy template to your feature directory
cp .specify/templates/implementation-verification.md \
   specs/XXX-your-feature/verification.md

# 2. Customize for your feature
vim specs/XXX-your-feature/verification.md
# Add feature-specific items

# 3. Track progress during implementation
# Check off items as you complete them

# 4. Pre-PR verification
# Ensure all items are checked or N/A

# 5. Create PR with verification summary
gh pr create --title "..." --body "..."
```

**Key Questions**:
- ✅ Do all tests pass?
- ✅ Are all acceptance criteria met?
- ✅ Are Constitutional standards upheld?
- ✅ Is code quality verified (clippy, fmt, docs)?
- ✅ Are performance targets met?

---

## 📊 Checklist Coverage

### Requirements Quality Checklist (208 items)

| Category | Items | Focus |
|----------|-------|-------|
| **Constitution Gates** | 5 | Simplicity, Anti-Abstraction, Integration-First, Performance |
| **Core Principles** | 7 | Zero Config, Single Binary, OpenAI-Compatible, etc. |
| **Technical Constraints** | 7 | Rust, Tokio, Axum, reqwest, DashMap, tracing, thiserror |
| **Spec Structure** | 51 | Metadata, User Stories, FRs, NFRs, Edge Cases, etc. |
| **Spec Clarity** | 8 | Quantification, ambiguity detection |
| **Architecture** | 14 | Framework patterns, state management |
| **Testing** | 14 | TDD workflow, test types, CI/CD |
| **Traceability** | 16 | ID consistency, priorities, cross-references |
| **NFR Quality** | 18 | Performance, resources, concurrency |
| **Edge Cases** | 17 | Error scenarios, boundary conditions, recovery |
| **Dependencies** | 12 | Dependencies, assumptions |
| **Ambiguities** | 15 | Conflicts, gaps, clarifications needed |
| **Documentation** | 12 | Structure, readability |
| **Final Validation** | 12 | Completeness, readiness |

### Implementation Verification Checklist (210 items)

| Category | Items | Focus |
|----------|-------|-------|
| **Acceptance Criteria** | 8 | AC completion, traceability |
| **TDD Compliance** | 17 | RED → GREEN → Refactor workflow |
| **Constitutional Compliance** | 19 | All 4 gates verified in code |
| **Code Quality** | 18 | Clippy, fmt, docs, no unsafe |
| **Functional Correctness** | 13 | All FRs and user stories implemented |
| **NFR Verification** | 19 | Performance, concurrency, reliability |
| **Edge Cases** | 15 | All edge cases from spec implemented |
| **Integration** | 12 | Dependencies, registry, router |
| **Configuration/CLI** | 13 | Config parsing, CLI commands |
| **Security & Safety** | 10 | Memory safety, input validation |
| **Documentation** | 7 | README, ARCHITECTURE, spec updates |
| **CI/CD** | 13 | CI checks, build, git hygiene |
| **Manual Testing** | 16 | Smoke tests, integration tests |
| **Compatibility** | 8 | OpenAI clients, backend compatibility |
| **Regression** | 4 | No regressions in existing features |
| **Final Sign-Off** | 18 | All checks complete, ready to merge |

---

## 🎯 Critical Items (Must Not Skip)

### Requirements Quality - Top 10

| ID | Item | Why Critical |
|----|------|--------------|
| CHK001 | Simplicity Gate checked? | Prevents over-engineering |
| CHK006 | Zero Configuration addressed? | Core Nexus principle |
| CHK078 | FR IDs sequential? | Traceability |
| CHK089 | Given/When/Then scenarios? | Testability |
| CHK123 | Performance targets quantified? | Measurable success |
| CHK145 | Edge cases documented? | Robust implementation |
| CHK158 | TDD workflow documented? | Constitutional requirement |
| CHK174 | Test types specified? | Coverage assurance |
| CHK185 | Dependencies documented? | Integration planning |
| CHK208 | Spec ready for implementation? | Final gate |

### Implementation Verification - Top 10

| ID | Item | Why Critical |
|----|------|--------------|
| VER-001 | All AC in tasks.md checked? | Completeness proof |
| VER-009 | TDD workflow followed? | Constitutional compliance |
| VER-040 | Performance gate met? | Non-negotiable target |
| VER-046 | Clippy passes? | Code quality |
| VER-082 | Concurrent stress tests pass? | Production readiness |
| VER-193 | All AC verified? | Feature completeness |
| VER-194 | cargo test passes? | Correctness |
| VER-200 | Ready for merge? | Final gate |
| VER-206 | Resilient principle upheld? | System reliability |
| VER-208 | Author sign-off? | Accountability |

---

## ⚡ Common Mistakes & Fixes

### Requirements Quality Phase

| Mistake | Fix |
|---------|-----|
| ❌ Vague NFRs ("fast", "efficient") | ✅ Quantify with metrics ("< 1ms", "< 50MB") |
| ❌ Missing edge cases | ✅ Document boundary conditions, error scenarios |
| ❌ Incomplete user stories | ✅ Add ≥2 Given/When/Then scenarios per story |
| ❌ No test strategy | ✅ Specify unit, integration, property-based tests |
| ❌ Gaps in FR numbering | ✅ Sequential IDs: FR-001, FR-002, FR-003... |

### Implementation Verification Phase

| Mistake | Fix |
|---------|-----|
| ❌ Tests written after implementation | ✅ Follow TDD: tests first, fail, implement, pass |
| ❌ Unchecked AC in tasks.md | ✅ Check all `[ ]` to `[x]` before PR |
| ❌ No benchmark for performance targets | ✅ Add criterion benchmarks, measure results |
| ❌ println! debugging left in code | ✅ Use tracing macros only |
| ❌ Unwrap/expect in production code | ✅ Proper error handling with thiserror |

---

## 📚 File Locations

```
.specify/
├── checklists/
│   └── requirements-quality.md          # Generated checklist (208 items)
├── templates/
│   └── implementation-verification.md   # Template for feature-specific copy
├── README-CHECKLISTS.md                 # Comprehensive guide
└── EXAMPLE-CHECKLIST-USAGE.md           # Real-world example (F06)

specs/XXX-your-feature/
├── spec.md                              # Feature specification
├── plan.md                              # Implementation plan
├── tasks.md                             # Task breakdown with AC
└── verification.md                      # Copy of verification template (customized)
```

---

## 🔄 Workflow Summary

```
┌──────────────────┐
│   Write Spec     │ ← Reference requirements-quality.md
└────────┬─────────┘
         ↓
┌──────────────────┐
│  Self-Review     │ ← Check off requirements-quality.md items
└────────┬─────────┘
         ↓
┌──────────────────┐
│  Team Review     │ ← Reviewer uses requirements-quality.md
└────────┬─────────┘
         ↓
┌──────────────────┐
│  Spec Approved   │ ✅
└────────┬─────────┘
         ↓
┌──────────────────┐
│ Copy & Customize │ ← cp implementation-verification.md to feature dir
│  Verification    │
└────────┬─────────┘
         ↓
┌──────────────────┐
│  Write Tests     │ ← Check TDD items (VER-009..VER-013)
│  (RED phase)     │
└────────┬─────────┘
         ↓
┌──────────────────┐
│  Implement       │ ← Check feature items (VER-063..VER-120)
│  (GREEN phase)   │
└────────┬─────────┘
         ↓
┌──────────────────┐
│  Refactor        │ ← Check performance items (VER-040, VER-076)
└────────┬─────────┘
         ↓
┌──────────────────┐
│  Pre-PR Check    │ ← Verify ALL 210 items checked or N/A
└────────┬─────────┘
         ↓
┌──────────────────┐
│  Create PR       │ ← Include verification summary in PR description
└────────┬─────────┘
         ↓
┌──────────────────┐
│  Code Review     │ ← Reviewer spot-checks critical items
└────────┬─────────┘
         ↓
┌──────────────────┐
│  Merge to Main   │ ✅
└──────────────────┘
```

---

## 💡 Pro Tips

1. **Requirements checklist**: Keep open in split window while writing spec
2. **Self-review first**: Catch 80% of issues before team review
3. **Incremental verification**: Check items as you go, not all at end
4. **Customize verification**: Add feature-specific items (VER-300+)
5. **Git history**: TDD workflow should be visible in commits
6. **Benchmark performance**: Don't guess, measure with criterion
7. **Manual smoke tests**: Actually run the feature end-to-end
8. **Update checklists**: Add items when you find gaps
9. **Mark N/A explicitly**: If item doesn't apply, explain why
10. **Trust but verify**: Checked items should be accurate, spot-check in review

---

## 🆘 Quick Help

### "Which checklist should I use?"

- **Writing a spec?** → Use **requirements-quality.md**
- **Writing code?** → Use **implementation-verification.md**
- **Reviewing a spec?** → Use **requirements-quality.md**
- **Reviewing code?** → Use **implementation-verification.md**

### "How many items should I check?"

- **Requirements quality**: Aim for 100% or justify N/A
- **Implementation verification**: Aim for 100% or justify N/A
- **Minimum**: >80% for each major section

### "What if an item doesn't apply?"

Mark as `[N/A]` with brief explanation:
```markdown
- [N/A] CHK071: API endpoint specs → This feature has no API endpoints
- [N/A] VER-121: Config file parsing → This feature uses defaults only
```

### "How do I customize the verification checklist?"

```bash
# 1. Copy template
cp .specify/templates/implementation-verification.md \
   specs/XXX-feature/verification.md

# 2. Add feature-specific items at end
vim specs/XXX-feature/verification.md

# Add section:
## Section 17: Feature-Specific Verification
- [ ] VER-300: mDNS discovery works for _ollama._tcp.local
- [ ] VER-301: Grace period prevents backend flapping
...
```

---

## 📞 Need More Help?

- **Comprehensive Guide**: `.specify/README-CHECKLISTS.md`
- **Real-World Example**: `.specify/EXAMPLE-CHECKLIST-USAGE.md`
- **Constitution**: `.specify/memory/constitution.md`
- **Copilot Instructions**: `.github/copilot-instructions.md`

---

**Version**: 1.0.0  
**Last Updated**: 2026-02-03  
**Print this card**: Keep at your desk during feature development!
