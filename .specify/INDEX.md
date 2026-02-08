# Nexus Quality Assurance System - Index

Welcome to the Nexus **three-checklist system** for ensuring feature quality throughout the development lifecycle.

## 🎯 What is This?

This is a comprehensive quality assurance system consisting of **three complementary checklists** that work together to ensure specification quality, implementation correctness, and acceptance criteria tracking.

## 📋 The Three Checklists

### 1. Requirements Validation Checklist (65 items)
**Purpose**: Quality gate for specifications - validates spec is ready BEFORE implementation  
**File**: `.specify/templates/requirements-validation.md` (copy to feature folder)  
**When**: After writing spec.md, plan.md, tasks.md - before creating feature branch  
**Who**: Spec authors

**What it validates**:
- ✅ Constitution gates are addressed
- ✅ Core principles are followed
- ✅ Spec is complete and clear
- ✅ Requirements are testable
- ✅ Edge cases are documented

---

### 2. Implementation Verification Checklist (210 items)
**Purpose**: Verify implementation correctness AFTER development  
**File**: `.specify/templates/implementation-verification.md` (copy to feature folder)  
**When**: During and after implementation, before PR merge  
**Who**: Feature implementers, code reviewers

**What it validates**:
- ✅ All acceptance criteria are met
- ✅ Tests pass and provide adequate coverage
- ✅ Code quality standards are met
- ✅ Constitutional principles are upheld in code
- ✅ Manual testing confirms correct behavior

---

### 3. Tasks & Acceptance Criteria (tasks.md)
**Purpose**: Track granular acceptance criteria during implementation  
**File**: `specs/XXX-feature/tasks.md` (created during spec phase)  
**When**: During implementation - check off as you complete each item  
**Who**: Feature implementers

**What it tracks**:
- ✅ Individual task completion
- ✅ Acceptance criteria per task
- ✅ Test requirements per task

---

## 🚀 Quick Start

### For Spec Authors

```bash
# 1. Write your spec
vim specs/XXX-your-feature/spec.md
vim specs/XXX-your-feature/plan.md
vim specs/XXX-your-feature/tasks.md

# 2. Copy requirements validation template
cp .specify/templates/requirements-validation.md specs/XXX-your-feature/requirements-validation.md

# 3. Complete the validation checklist
# Fix any issues before proceeding

# 4. Verify spec is ready (should be 0 unchecked)
grep -c "\- \[ \]" specs/XXX-your-feature/requirements-validation.md
```

### For Implementers

```bash
# 1. Copy verification template to your feature directory
cp .specify/templates/implementation-verification.md \
   specs/XXX-your-feature/verification.md

# 2. Implement using TDD - check off tasks.md criteria as you go

# 3. Complete verification checklist after implementation

# 4. Pre-PR verification (all should be 0)
grep -c "\- \[ \]" specs/XXX-your-feature/verification.md
grep -c "\- \[ \]" specs/XXX-your-feature/tasks.md
```

---

## 📚 Documentation

Choose your learning path:

### New to the System? Start Here:
1. **[QUICK-REFERENCE.md](.specify/QUICK-REFERENCE.md)** - One-page overview (261 lines)
   - Quick start guides
   - Top 10 critical items
   - Common mistakes and fixes
   - Daily reference card

### Want Complete Understanding? Read:
2. **[README-CHECKLISTS.md](.specify/README-CHECKLISTS.md)** - Comprehensive guide (422 lines)
   - Complete explanation of both checklists
   - When to use each
   - Key differences
   - Integration with speckit agents
   - Best practices
   - FAQ

### Want a Real-World Example? See:
3. **[EXAMPLE-CHECKLIST-USAGE.md](.specify/EXAMPLE-CHECKLIST-USAGE.md)** - Walkthrough (688 lines)
   - F06 Intelligent Router feature example
   - Phase-by-phase usage throughout feature lifecycle
   - Exact commands and checklist updates
   - Findings and retrospective
   - Continuous improvement

---

## 🗂️ File Locations

```
.specify/
├── checklists/
│   └── requirements-quality.md              # Requirements quality (208 items)
│
├── templates/
│   └── implementation-verification.md       # Verification template (210 items)
│
├── INDEX.md                                 # This file (you are here)
├── QUICK-REFERENCE.md                       # One-page reference card
├── README-CHECKLISTS.md                     # Comprehensive guide
└── EXAMPLE-CHECKLIST-USAGE.md               # Real-world example
```

---

## 🔄 Workflow Overview

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
│ Copy Verification│ ← cp implementation-verification.md to feature
│   Template       │
└────────┬─────────┘
         ↓
┌──────────────────┐
│  Implement       │ ← Check verification items as you go
│  Feature         │
└────────┬─────────┘
         ↓
┌──────────────────┐
│  Pre-PR Check    │ ← Verify ALL items checked or N/A
└────────┬─────────┘
         ↓
┌──────────────────┐
│  Create PR       │ ← Include verification summary
└────────┬─────────┘
         ↓
┌──────────────────┐
│  Code Review     │ ← Reviewer spot-checks verification items
└────────┬─────────┘
         ↓
┌──────────────────┐
│  Merge to Main   │ ✅
└──────────────────┘
```

---

## 🎓 Learning Path by Role

### Spec Authors
1. Start with **QUICK-REFERENCE.md** (Quick Start for Spec Authors)
2. Open **requirements-quality.md** while writing your first spec
3. Read **README-CHECKLISTS.md** section "For Spec Authors"
4. Review **EXAMPLE-CHECKLIST-USAGE.md** Phase 1-2 (Spec Writing & Review)

### Spec Reviewers
1. Start with **QUICK-REFERENCE.md** (Quick Start for Spec Authors)
2. Use **requirements-quality.md** as systematic review guide
3. Read **README-CHECKLISTS.md** section "For Spec Reviewers"
4. Review **EXAMPLE-CHECKLIST-USAGE.md** Day 4 (Team Review)

### Feature Implementers
1. Start with **QUICK-REFERENCE.md** (Quick Start for Implementers)
2. Copy **implementation-verification.md** template and customize
3. Read **README-CHECKLISTS.md** section "For Implementers"
4. Follow **EXAMPLE-CHECKLIST-USAGE.md** Phase 3-4 (Implementation & Verification)

### Code Reviewers
1. Start with **QUICK-REFERENCE.md** (Critical Items)
2. Use implementer's **verification.md** to spot-check
3. Read **README-CHECKLISTS.md** section "For Code Reviewers"
4. Review **EXAMPLE-CHECKLIST-USAGE.md** Phase 5 (PR & Code Review)

---

## 💡 Key Concepts

### "Unit Tests for Requirements"
The requirements quality checklist is like unit tests for your specification. It validates that requirements are well-written, not that code is correct.

**Example**:
- ❌ Wrong: "Verify button clicks work"
- ✅ Right: "Are button interaction requirements specified with hover/focus/active states?"

### "Implementation Correctness Verification"
The implementation verification checklist validates that code meets all requirements and standards.

**Example**:
- ✅ "All acceptance criteria in tasks.md checked?"
- ✅ "Routing decision < 1ms (benchmark: 450ns)?"
- ✅ "Constitutional gates upheld in code?"

### Complementary, Not Redundant
The two checklists work together:
- **Requirements Quality**: Tests if spec is ready for implementation
- **Implementation Verification**: Tests if implementation is ready for merge

---

## 📊 Statistics

| Metric | Requirements Quality | Implementation Verification |
|--------|----------------------|----------------------------|
| **Total Items** | 208 | 210 |
| **Categories** | 14 | 16 |
| **Lines** | 465 | 480 |
| **Constitutional Items** | 19 (gates + principles) | 26 (gates + principles + verification) |
| **Traceability** | >80% | 100% AC tracking |

---

## 🔍 What Each Checklist Covers

### Requirements Quality Checklist (14 Categories)
1. Constitution Gates (5 items) - Simplicity, Anti-Abstraction, Integration-First, Performance
2. Core Principles (7 items) - Zero Config, Single Binary, OpenAI-Compatible, etc.
3. Technical Constraints (7 items) - Rust, Tokio, Axum, reqwest, etc.
4. Spec Structure (51 items) - Metadata, User Stories, FRs, NFRs, etc.
5. Spec Clarity (8 items) - Quantification, ambiguity detection
6. Architecture (14 items) - Framework patterns, state management
7. Testing (14 items) - TDD workflow, test types
8. Traceability (16 items) - ID consistency, priorities, scenarios
9. NFR Quality (18 items) - Performance, resources, concurrency
10. Edge Cases (17 items) - Error scenarios, recovery
11. Dependencies (12 items) - Dependencies, assumptions
12. Ambiguities (15 items) - Conflicts, gaps
13. Documentation (12 items) - Structure, readability
14. Final Validation (12 items) - Completeness, readiness

### Implementation Verification Checklist (16 Categories)
1. Acceptance Criteria (8 items) - AC completion, traceability
2. TDD Compliance (17 items) - RED → GREEN → Refactor
3. Constitutional Compliance (19 items) - All gates and principles
4. Code Quality (18 items) - Clippy, fmt, docs
5. Functional Correctness (13 items) - All FRs implemented
6. NFR Verification (19 items) - Performance, concurrency
7. Edge Cases (15 items) - All edge cases implemented
8. Integration (12 items) - Dependencies work
9. Configuration (13 items) - Config parsing, CLI
10. Security (10 items) - Memory safety, input validation
11. Documentation (7 items) - README, ARCHITECTURE updates
12. CI/CD (13 items) - CI checks, build, git
13. Manual Testing (16 items) - Smoke tests, integration
14. Compatibility (8 items) - OpenAI clients, backends
15. Regression (4 items) - No existing feature broken
16. Final Sign-Off (18 items) - All checks complete

---

## 🆘 Getting Help

### Quick Questions?
→ Check **QUICK-REFERENCE.md** first

### Need Detailed Explanation?
→ See **README-CHECKLISTS.md** FAQ section

### Want to See It In Action?
→ Follow **EXAMPLE-CHECKLIST-USAGE.md**

### Still Stuck?
→ Ask in team chat with reference to specific checklist item (e.g., "CHK078" or "VER-040")

---

## 🔄 Continuous Improvement

These checklists are living documents. After each feature:

1. **Retrospective**: What did checklists catch? What did they miss?
2. **Update**: Add items for missed issues, remove outdated items
3. **Refine**: Improve clarity, examples, and organization
4. **Share learnings**: Update documentation with new patterns

See **README-CHECKLISTS.md** section "Continuous Improvement" for details.

---

## 🎯 Success Criteria

You're using the checklists effectively when:
- ✅ Specs catch 80%+ issues before implementation
- ✅ Implementation passes verification on first try
- ✅ Code reviews focus on design, not missing basics
- ✅ PRs merge faster with fewer back-and-forth iterations
- ✅ Features ship with fewer bugs
- ✅ Team has shared understanding of quality standards

---

## 📝 Version History

| Version | Date | Changes |
|---------|------|---------|
| 1.0.0 | 2026-02-03 | Initial release with dual checklist system |

**Next Review**: After 5 features using this system (target: 2026-03-01)

---

## 🚀 Ready to Start?

1. **Writing a spec?** → Open **requirements-quality.md** in split window
2. **Implementing a feature?** → Copy **implementation-verification.md** and customize
3. **New to the system?** → Read **QUICK-REFERENCE.md** first
4. **Want comprehensive guide?** → Read **README-CHECKLISTS.md**

---

**Remember**: These checklists exist to help you ship high-quality features faster, not to slow you down. Use them as a guide, not a bureaucratic burden. When in doubt, ask yourself: "Does this help ensure quality?" If yes, do it. If no, skip it and document why.

---

**Last Updated**: 2026-02-03  
**Maintained By**: Nexus Development Team  
**Questions?** See **README-CHECKLISTS.md** FAQ or ask in team chat
