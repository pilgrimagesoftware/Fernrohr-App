# Architecture Decision Records

This directory contains Architecture Decision Records (ADRs) for service-local decisions specific to the Fernrohr App repository. Platform-wide ADRs (prefixed `PADR-*`) live in the parent Fernrohr repository.

## What is an ADR?

An ADR documents a significant architectural or design decision made in the project. It captures:

- The context and problem
- The decision made
- Consequences and trade-offs
- Alternatives considered

## Status Values

- **Proposed:** Under discussion or awaiting approval
- **Accepted:** Decision is approved and being implemented
- **Deprecated:** No longer recommended but not yet superseded
- **Superseded:** Replaced by a newer ADR

## Writing an ADR

1. Copy the template from `0000-template.md`
2. Assign the next available `ADR-NNNN` number (this repo's local tier)
3. Fill in all sections with clear, concise reasoning
4. Submit as part of a PR for discussion
5. Move status to "Accepted" once approved

## Records

| Title | Status |
|-------|--------|
| [Template](0000-template.md) | Template |

## Platform ADRs

Platform-wide decisions (prefixed `PADR-*`) are documented in the parent Fernrohr repository at `docs/adr/README.md`. These are binding constraints - consult them before proposing changes that might contradict an accepted record.
