# Requirements Validation Checklist

**Purpose**: Validate spec quality BEFORE implementation begins  
**Type**: Requirements Quality Gate  
**Created**: [Date]  
**Feature**: [Feature Name/ID]  
**Last Updated**: [Date]

---

## How to Use

1. Complete this checklist after writing spec.md, plan.md, and tasks.md
2. Mark `[x]` for items that pass
3. Mark `[-]` for items not applicable to this feature
4. Fix any `[ ]` items before proceeding to implementation
5. Goal: 0 unchecked items before creating feature branch

---

## Section 1: Constitution Gates (Mandatory)

All gates must be explicitly addressed in the specification.

- [ ] REQ-001: **Simplicity Gate** checked? (≤3 main modules, no speculative features, simplest approach)
- [ ] REQ-002: **Anti-Abstraction Gate** checked? (Direct framework use, no wrapper layers)
- [ ] REQ-003: **Integration-First Gate** checked? (API contracts defined, integration tests planned)
- [ ] REQ-004: **Performance Gate** checked? (Routing <1ms, overhead <5ms, memory <50MB)

---

## Section 2: Core Principles Alignment

- [ ] REQ-005: **Zero Configuration** - Feature works with sensible defaults?
- [ ] REQ-006: **Single Binary** - No new runtime dependencies added?
- [ ] REQ-007: **OpenAI-Compatible** - API matches OpenAI format (if applicable)?
- [ ] REQ-008: **Backend Agnostic** - No backend-specific assumptions in core logic?
- [ ] REQ-009: **Intelligent Routing** - Considers capabilities before load/latency (if applicable)?
- [ ] REQ-010: **Resilience** - Handles failures gracefully, no crashes on errors?
- [ ] REQ-011: **Local-First** - Works offline, no external dependencies?

---

## Section 3: Specification Completeness

### Metadata
- [ ] REQ-012: Feature ID and branch name specified?
- [ ] REQ-013: Priority assigned (P0/P1/P2)?
- [ ] REQ-014: Dependencies on other features documented?

### Overview
- [ ] REQ-015: Goals explicitly listed?
- [ ] REQ-016: Non-Goals explicitly listed (scope boundaries)?
- [ ] REQ-017: Feature purpose stated clearly in 1-2 sentences?

### User Stories
- [ ] REQ-018: User stories in standard format? ("As a [role], I want [goal] so that [benefit]")
- [ ] REQ-019: Each user story has priority and rationale?
- [ ] REQ-020: Acceptance scenarios in Given/When/Then format?
- [ ] REQ-021: Both happy path and error scenarios covered?

### Technical Design
- [ ] REQ-022: API contracts defined (endpoints, request/response types)?
- [ ] REQ-023: Data structures defined with field types?
- [ ] REQ-024: State management approach documented?
- [ ] REQ-025: Error handling strategy defined?

---

## Section 4: Requirements Quality

### Clarity
- [ ] REQ-026: All requirements are quantified (no vague terms like "fast", "many")?
- [ ] REQ-027: No ambiguous terms ("should", "might", "could" → use "must", "will")?
- [ ] REQ-028: Technical jargon is defined or referenced?

### Testability
- [ ] REQ-029: Each requirement can be verified with a test?
- [ ] REQ-030: Success/failure criteria are measurable?
- [ ] REQ-031: Edge cases identified and documented?

### Consistency
- [ ] REQ-032: No conflicting requirements exist?
- [ ] REQ-033: Terminology is used consistently throughout?
- [ ] REQ-034: Priority levels are consistent with project roadmap?

---

## Section 5: Testing Strategy

- [ ] REQ-035: Unit test approach documented?
- [ ] REQ-036: Integration test approach documented?
- [ ] REQ-037: Property-based tests planned for complex logic?
- [ ] REQ-038: Test data/mocks strategy defined?
- [ ] REQ-039: Estimated test count provided?

---

## Section 6: Non-Functional Requirements

### Performance
- [ ] REQ-040: Latency targets specified?
- [ ] REQ-041: Memory limits specified?
- [ ] REQ-042: Throughput requirements specified (if applicable)?

### Concurrency
- [ ] REQ-043: Thread safety requirements documented?
- [ ] REQ-044: Concurrent access patterns identified?

### Configuration
- [ ] REQ-045: New config options documented?
- [ ] REQ-046: Environment variable overrides defined?
- [ ] REQ-047: Default values specified?

---

## Section 7: Edge Cases & Error Handling

- [ ] REQ-048: Empty/null input handling defined?
- [ ] REQ-049: Maximum value handling defined?
- [ ] REQ-050: Network failure handling defined?
- [ ] REQ-051: Invalid input handling defined?
- [ ] REQ-052: Concurrent modification handling defined?

---

## Section 8: Dependencies & Assumptions

- [ ] REQ-053: External crate dependencies listed?
- [ ] REQ-054: Feature dependencies (F01, F02, etc.) listed?
- [ ] REQ-055: Assumptions explicitly stated?
- [ ] REQ-056: Risks identified?

---

## Section 9: Documentation

- [ ] REQ-057: README updates planned (if user-facing)?
- [ ] REQ-058: ARCHITECTURE.md updates planned (if architecture changes)?
- [ ] REQ-059: Config example updates planned (if new config options)?
- [ ] REQ-060: Walkthrough planned for complex implementations?

---

## Section 10: Final Validation

- [ ] REQ-061: Spec reviewed for completeness?
- [ ] REQ-062: Plan reviewed for feasibility?
- [ ] REQ-063: Tasks are atomic and independently testable?
- [ ] REQ-064: Acceptance criteria are clear and measurable?
- [ ] REQ-065: Ready for implementation (no blockers)?

---

## Validation Summary

| Section | Total | Checked | N/A | Unchecked |
|---------|-------|---------|-----|-----------|
| Constitution Gates | 4 | | | |
| Core Principles | 7 | | | |
| Spec Completeness | 14 | | | |
| Requirements Quality | 9 | | | |
| Testing Strategy | 5 | | | |
| NFRs | 8 | | | |
| Edge Cases | 5 | | | |
| Dependencies | 4 | | | |
| Documentation | 4 | | | |
| Final Validation | 5 | | | |
| **Total** | **65** | | | |

**Validation Result**: [ ] PASS - Ready for implementation / [ ] FAIL - Issues to resolve

---

## Notes

_Document any issues found, decisions made, or items deferred:_

---

## Version History

| Version | Date | Changes | Author |
|---------|------|---------|--------|
| 1.0.0 | 2026-02-08 | Initial template | - |
