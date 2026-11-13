# ADR-0002: Helm release actions run the user's helm CLI

**Status:** Proposed

**Date:** 2026-11-13

## Context

Helm Releases (#370, the meta repo's `helm-releases` change) lists a cluster's releases and offers
Rollback and Uninstall. A release lives in the cluster as one Secret per revision, but acting on it
is much more than rewriting those Secrets. A rollback renders the target revision's manifest and
applies it with Helm's three-way merge against the live objects. It runs the chart's pre- and
post-hooks in weight order and waits on them, records a new revision, and supersedes the old ones.
An uninstall runs delete hooks, deletes resources in Helm's order, honours resource policies such as
`helm.sh/resource-policy: keep`, and removes or keeps history. A mismatch in any of that corrupts
the release's history for every other Helm user of the cluster.

Helm itself is a Go program. Its engine isn't available as a Rust library, and linking it would add
a Go toolchain and FFI to the build.

## Decision

Rollback and Uninstall run the user's own `helm` 3 binary: `helm rollback <name> <revision> -n
<namespace>` and `helm uninstall <name> -n <namespace>`. Reading releases (the list, detail,
history, values and manifest) stays native, decoded from the release Secrets.

The runner is `app/src/helm_cli/`:

- **Finding helm** (`locate`): the path in Settings → Tools if set, otherwise `helm` on the login
  shell's PATH, the same resolution the command tunnels use. The binary is accepted only when
  `helm version --short` reports `v3.`. The answer is checked off the UI thread, cached, and
  checked again when the Settings path changes, so an action's availability never runs a process
  on a render path.
- **Reaching the cluster** (`kubeconfig`): each run writes a kubeconfig holding one context, the
  context's own. Its cluster is the connection's effective endpoint: as the kubeconfig has it for
  a direct connection; `https://127.0.0.1:<port>` with `tls-server-name` set to the real host
  through an SSH tunnel or a forward-mode command tunnel; the real server with `proxy-url` set to
  the tunnel through a proxy-mode command tunnel. The user stanza is copied unchanged (exec
  plugins, tokens, client certificates), so `helm` authenticates exactly as the context does. The
  file is created 0600 under the cache dir and removed when the run ends. Startup removes any a
  crash left behind. The user's kubeconfig and environment are never changed.
- **Running** (`run`, `start`): no shell, `helm` in its own process group with the login shell's
  PATH, stdout and stderr captured, and a ten-minute timeout (`HELM_ACTION_TIMEOUT`) after which
  the group is killed and the run reported failed. A failure reports stderr's last lines. One run
  per release at a time; a second is refused as already running. The connection, and so its
  tunnel, is held for the run's length.

## Consequences

- Rollback and Uninstall behave exactly as they do from a terminal, hooks and history included,
  and stay correct as Helm changes, with no Helm logic of our own to keep in step.
- The actions need Helm 3 installed. Without it they are shown disabled with the reason, and
  everything read-only still works.
- `helm` sees a copy of the context's credentials in a temporary file for the length of a run.
  The file is private to the user and short-lived, but a crash leaves it until the next start.
- Tests use a fake `helm` script: arguments, environment, failures, timeouts and the kubeconfig
  are covered without a real Helm or cluster. Behaviour against a real Helm is checked by hand
  (the change's section 9).

## Alternatives Considered

- **Reimplement rollback and uninstall in Rust.** Rejected. Helm's merge, hook ordering, waiting
  and history bookkeeping are large and subtle, and a mismatch damages releases other tools rely
  on.
- **Link Helm's Go code through FFI.** Rejected. It adds a Go toolchain to every build and
  platform package, and Helm's Go API isn't stable.
- **Edit the release Secrets directly** (e.g. mark an older revision deployed). Rejected. It
  changes the record without changing the cluster, leaving them inconsistent.
- **Pass the user's kubeconfig and `--kube-context` as is.** Rejected. A tunneled context only
  works through the app's tunnel, which that kubeconfig doesn't know about.
