# Nexus Feature Specifications

This directory contains feature specifications created via spec-driven development.

## Structure

Each feature gets its own directory:

```
specs/
├── 001-core-api-gateway/
│   ├── spec.md           # Feature specification
│   ├── plan.md           # Implementation plan
│   ├── research.md       # Technology research
│   ├── data-model.md     # Data structures
│   ├── contracts/        # API contracts
│   ├── quickstart.md     # Key validation scenarios
│   └── tasks.md          # Implementation tasks
├── 002-backend-registry/
│   └── ...
└── ...
```

## Workflow

1. `/speckit.specify` - Create feature spec
2. `/speckit.plan` - Create implementation plan
3. `/speckit.tasks` - Generate task list
4. `/speckit.implement` - Execute tasks

See `docs/SPEC_KIT_PROMPTS.md` for detailed prompts.
