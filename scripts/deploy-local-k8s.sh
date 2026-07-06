#!/usr/bin/env bash
# Deploy to the local Kubernetes cluster on ioachim-minipc.
set -euo pipefail

TAG="${1:-$(git rev-parse --short HEAD)}"
IMAGE="${IMAGE:-ghcr.io/lihor-hub/romanian-folk-fight}:${TAG}"

echo "→ Deploying romanian-folk-fight (image=${IMAGE})"

kubectl create namespace romanian-folk-fight --dry-run=client -o yaml | kubectl apply -f -

# The deployment pulls from a private GHCR package, so the namespace needs a
# docker-registry secret. Refresh it when a token is provided; otherwise the
# existing secret must already be in place.
if [[ -n "${GHCR_TOKEN:-}" ]]; then
  kubectl -n romanian-folk-fight create secret docker-registry ghcr-pull-secret \
    --docker-server=ghcr.io \
    --docker-username="${GHCR_USERNAME:-lihor-hub}" \
    --docker-password="${GHCR_TOKEN}" \
    --dry-run=client -o yaml | kubectl apply -f -
elif ! kubectl -n romanian-folk-fight get secret ghcr-pull-secret >/dev/null 2>&1; then
  echo "✗ Secret ghcr-pull-secret not found and GHCR_TOKEN is not set." >&2
  echo "  Export GHCR_TOKEN (a PAT with read:packages) and optionally GHCR_USERNAME, then re-run." >&2
  exit 1
fi

# Apply manifests
kubectl apply -f deploy/deployment.yaml
kubectl apply -f deploy/service.yaml

# Set deployment image to the correct tag
kubectl -n romanian-folk-fight set image deployment/romanian-folk-fight romanian-folk-fight="${IMAGE}"

echo "→ Waiting for app rollout"
kubectl -n romanian-folk-fight rollout status deploy/romanian-folk-fight
echo "✓ Done"
