#!/usr/bin/env bash
# helm-releases section 9: runs the ignored Helm integration test against a
# throwaway LOCAL minikube profile, then deletes it.
#
# Safety: KUBECONFIG points at a temp file for the whole run, so minikube
# writes its context there - never into ~/.kube/config, and the user's current
# context is untouched. Every helm call also names the context explicitly. The
# profile is deleted on any exit, including failure or Ctrl-C.
#
# Usage: scripts/helm-integration.sh   (needs minikube, docker, helm, cargo)
set -euo pipefail

PROFILE="${FERNROHR_HELM_IT_PROFILE:-fernrohr-helm-it}"
NAMESPACE="fernrohr-it"
RELEASE="it-web"
REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/fernrohr-helm-it.XXXXXX")"
export KUBECONFIG="$WORK/kubeconfig"

cleanup() {
  echo "==> deleting minikube profile $PROFILE"
  minikube delete -p "$PROFILE" >/dev/null 2>&1 || true
  rm -rf "$WORK"
}

for tool in minikube docker helm cargo; do
  command -v "$tool" >/dev/null || { echo "missing: $tool" >&2; exit 1; }
done
if minikube profile list -o json 2>/dev/null | grep -q "\"Name\":\"$PROFILE\""; then
  echo "minikube profile $PROFILE already exists; refusing to reuse or delete it" >&2
  exit 1
fi
trap cleanup EXIT INT TERM

echo "==> starting minikube profile $PROFILE (kubeconfig: $KUBECONFIG)"
minikube start -p "$PROFILE" --driver=docker --cpus=2 --memory=2g --wait=all
CONTEXT="$PROFILE"

echo "==> installing $RELEASE and upgrading it twice"
helm create "$WORK/demo" >/dev/null
for count in 1 2 3; do
  verb=upgrade
  [ "$count" = 1 ] && verb=install
  helm --kube-context "$CONTEXT" "$verb" "$RELEASE" "$WORK/demo" \
    --namespace "$NAMESPACE" --create-namespace \
    --set "replicaCount=$count" --wait --timeout 5m
done
helm --kube-context "$CONTEXT" history "$RELEASE" --namespace "$NAMESPACE"

echo "==> running the integration test"
cd "$REPO/app"
FERNROHR_HELM_IT_KUBECONFIG="$KUBECONFIG" FERNROHR_HELM_IT_CONTEXT="$CONTEXT" \
  cargo test helm_live -- --ignored --nocapture
echo "==> passed"
