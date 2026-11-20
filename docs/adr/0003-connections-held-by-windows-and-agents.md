# ADR-0003: Connections are held by windows and agent sessions

**Status:** Proposed

**Date:** 2026-11-20

## Context

A context's connection, its watches and its tunnel forward live in one `ClusterSession`, shared by
everything that uses the context. Until now its lifetime was a set of window ids: the session lived
while at least one window used the context, and closing or disconnecting the last such window shut
it down.

MCP clients (agents) use connections too (#367, the meta repo's `mcp-connections` change). An agent
that connected a context left it held only by the window it landed in, so closing that window cut
the agent off mid-task. The user also wants agent-only connections kept out of windows entirely.
That only works if something other than a window can keep a connection open.

## Decision

A session's holders are a set of `Holder` values: `Holder::Window(WindowId)` or
`Holder::Agent(AgentId)`. The session lives while the set is non-empty; the last release of either
kind shuts it down exactly as before. The first holder a session gets is recorded as the one that
connected it.

An `AgentId` is an in-app handle, unique for the app's lifetime and never reused. It is not the
agent's MCP session identifier: that identifier is a secret held by the MCP layer, which maps a live
session to its `AgentId`. Secrets never enter the session map.

Window behaviour is unchanged: `hold` and `release` take `impl Into<Holder>`, so window call sites
pass a `WindowId` as before. The count used for "stays connected in N other windows" counts windows
only (`window_holder_count`).

## Consequences

- An agent's connection survives the user closing windows, including the last one (quiet mode).
- Ownership stays on `ClusterSession`, so pause and resume (`_health`), watches and the tunnel
  forward need no change for agent-only holders.
- Anything that assumed "no window holds it" means "nobody uses it" is now wrong. The picker's retry
  (`ClusterRegistry::retry_with`) swaps in a new connection when no holder remains; it must redial
  in place instead (App#431's `redial`) so an agent isn't left on an orphaned entity.
- Code that shows or counts holders has to say which kind it means.

## Alternatives Considered

- **A separate agent refcount beside the window set.** Rejected: two counters to keep in step, and
  the Connections window needs holder identities, not a number.
- **Key agent holds by the MCP session identifier.** Rejected: the identifier is a secret
  (`mcp-connections` Security 4); spreading it through the session map would widen where it lives.
- **Keep windows as the only holders and hide agent windows.** Rejected: a hidden window still
  carries panels, watches and layout state, and closing every window would still disconnect.
