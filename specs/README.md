# Nexus Feature Specifications

This directory contains feature specifications created via spec-driven development.

## Structure

Each feature gets its own directory:

```
specs/
├── 001-backend-registry/
│   ├── spec.md           # Feature specification
│   ├── plan.md           # Implementation plan
│   └── tasks.md          # Implementation tasks
├── 002-health-checker/
│   └── ...
└── README.md
```

## Workflow

1. **Specify** - Create feature spec (`spec.md`)
2. **Plan** - Create implementation plan (`plan.md`)
3. **Tasks** - Generate task list (`tasks.md`)
4. **Issues** - Create GitHub issues from tasks
5. **Analyze** - Check spec/plan/tasks consistency
6. **Implement** - Execute tasks (TDD: tests first)

## GitHub Integration

Tasks are tracked as GitHub issues for collaboration:

```bash
# View all issues
gh issue list

# View specific issue
gh issue view N

# Close issue after completing task
gh issue close N

# Filter by feature label
gh issue list --label backend-registry
```

## Labels

| Label | Description |
|-------|-------------|
| `P0` | MVP priority |
| `backend-registry` | Backend Registry feature |
| `testing` | Testing-related tasks |
| `documentation` | Documentation tasks |
| `good first issue` | Good for new contributors |

See `docs/SPEC_KIT_PROMPTS.md` for detailed prompts and workflow guide.
