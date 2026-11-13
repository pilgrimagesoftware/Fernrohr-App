#!/usr/bin/env bash
# helm-releases section 9: runs the ignored Helm integration test against a
# throwaway LOCAL minikube profile, then deletes it.
#
# Safety: KUBECONFIG points at a temp file for the whole run, so minikube
# writes its context there - never into ~/.kube/config, and the user's current
# context is untouched. Every helm call also names the context explicitly. The
# profile is deleted on any exit, including failure or Ctrl-C - but only once
# this run has created it, so an existing profile is never touched.
#
# Usage: scripts/helm-integration.sh   (needs minikube, docker, helm, cargo)
set -euo pipefail

# Fixed on purpose: the cleanup deletes this profile, and the test refuses any
# context without this prefix.
PROFILE="fernrohr-helm-it"
NAMESPACE="fernrohr-it"
RELEASE="it-web"
REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/fernrohr-helm-it.XXXXXX")"
export KUBECONFIG="$WORK/kubeconfig"

CREATED=0
cleanup() {
  if [ "$CREATED" = 1 ]; then
    echo "==> deleting minikube profile $PROFILE"
    minikube delete -p "$PROFILE" >/dev/null 2>&1 || true
  fi
  rm -rf "$WORK"
}
trap cleanup EXIT INT TERM

for tool in minikube docker helm cargo; do
  command -v "$tool" >/dev/null || { echo "missing: $tool" >&2; exit 1; }
done
# `minikube status` exits 85 only when the profile doesn't exist. Any other
# result (running, stopped, or an error) means it may exist: refuse, so the
# cleanup can never delete a profile this run didn't create.
status=0
minikube status -p "$PROFILE" >/dev/null 2>&1 || status=$?
if [ "$status" != 85 ]; then
  echo "minikube profile $PROFILE may already exist (status exit $status); refusing to reuse or delete it" >&2
  exit 1
fi

echo "==> starting minikube profile $PROFILE (kubeconfig: $KUBECONFIG)"
CREATED=1
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
