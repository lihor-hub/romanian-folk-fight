#!/usr/bin/env bash
# Deploy to the local Kubernetes cluster on ioachim-minipc.
set -euo pipefail

TAG="${1:-$(git rev-parse --short HEAD)}"
IMAGE="${IMAGE:-ghcr.io/lihor-hub/romanian-folk-fight}:${TAG}"

echo "→ Deploying romanian-folk-fight (image=${IMAGE})"

kubectl create namespace romanian-folk-fight --dry-run=client -o yaml | kubectl apply -f -

# Apply manifests
kubectl apply -f deploy/deployment.yaml
kubectl apply -f deploy/service.yaml

# Set deployment image to the correct tag
kubectl -n romanian-folk-fight set image deployment/romanian-folk-fight romanian-folk-fight="${IMAGE}"

echo "→ Waiting for app rollout"
kubectl -n romanian-folk-fight rollout status deploy/romanian-folk-fight
echo "✓ Done"
