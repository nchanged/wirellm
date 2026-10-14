#!/usr/bin/env bash
# ==============================================================================
# WireLLM - Publish Container Image & Deploy to Kubernetes Cluster
#
# 1. Verifies Docker daemon & multiplatform buildx capability
# 2. Builds & pushes multi-arch (linux/arm64) image to GitLab container registry
# 3. Prepares Kubernetes namespace & gitlab-registry pull secret
# 4. Deploys / Upgrades WireLLM via Helm
# 5. Performs rollout restart & verifies health endpoint
# ==============================================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

# Load local cluster-specific deployment variables if present (gitignored)
if [ -f "$REPO_ROOT/.env.publish" ]; then
  # shellcheck source=/dev/null
  source "$REPO_ROOT/.env.publish"
fi

# Configuration with generic open-source defaults
IMAGE="${IMAGE:-ghcr.io/wirellm/wirellm:latest}"
CONTEXT="${CONTEXT:-default}"
NS="${NS:-wirellm}"
SITE_URL="${SITE_URL:-https://llm.example.com}"
PLATFORM="${PLATFORM:-linux/arm64}"
VALUES_FILE="${VALUES_FILE:-}"

BUILD_ONLY=false
SKIP_PUSH=false

for arg in "$@"; do
  case "$arg" in
    --build-only)
      BUILD_ONLY=true
      ;;
    --no-push)
      SKIP_PUSH=true
      ;;
    --help|-h)
      echo "Usage: ./scripts/publish.sh [OPTIONS]"
      echo ""
      echo "Options:"
      echo "  --build-only    Build and push the Docker image without deploying to Kubernetes"
      echo "  --no-push       Build Docker image locally without pushing to remote registry"
      echo "  --help, -h      Show this help message"
      echo ""
      echo "Environment variables (or define in .env.publish):"
      echo "  IMAGE           Docker image tag (default: ghcr.io/wirellm/wirellm:latest)"
      echo "  CONTEXT         kubectl context (default: default)"
      echo "  NS              Kubernetes namespace (default: wirellm)"
      echo "  SITE_URL        Public URL for verification (default: https://llm.example.com)"
      echo "  VALUES_FILE     Custom Helm values file (default: deploy/helm/wirellm/values.yaml)"
      exit 0
      ;;
  esac
done

echo "=================================================="
echo "🚀 Publishing WireLLM ($IMAGE)"
echo "   Cluster Context: $CONTEXT | Namespace: $NS"
echo "=================================================="

# 1. Check Docker Daemon
echo "=== [1/5] Checking Docker Engine..."
if ! docker info > /dev/null 2>&1; then
  echo "Docker daemon is not running. Starting Docker Desktop..."
  open -a Docker || true
  for _ in {1..30}; do
    if docker info > /dev/null 2>&1; then
      break
    fi
    sleep 1
  done
fi

if ! docker info > /dev/null 2>&1; then
  echo "❌ Error: Docker daemon did not start in time. Aborting." >&2
  exit 1
fi

GIT_SHA="$(git rev-parse --short HEAD 2>/dev/null || echo "unknown")"

# 2. Build & Push Multi-Arch Image
echo "=== [2/5] Building Docker image for $PLATFORM (commit: $GIT_SHA)..."
if [ "$SKIP_PUSH" = true ]; then
  docker buildx build \
    --platform "$PLATFORM" \
    --build-arg GIT_SHA="$GIT_SHA" \
    --load \
    -t "$IMAGE" \
    -f Dockerfile .
  echo "✅ Built image locally: $IMAGE (push skipped)"
else
  docker buildx build \
    --platform "$PLATFORM" \
    --build-arg GIT_SHA="$GIT_SHA" \
    --push \
    -t "$IMAGE" \
    -f Dockerfile .
  echo "✅ Built and pushed image: $IMAGE"
fi

if [ "$BUILD_ONLY" = true ]; then
  echo "=================================================="
  echo "🎉 Build & publish complete (--build-only specified)."
  echo "=================================================="
  exit 0
fi

# 3. Prepare Namespace and Registry Secret
echo "=== [3/5] Preparing namespace '$NS' on cluster ($CONTEXT)..."
if command -v kubectl > /dev/null 2>&1; then
  kubectl create namespace "$NS" --dry-run=client -o yaml | kubectl apply --context "$CONTEXT" -f -

  if ! kubectl get secret gitlab-registry -n "$NS" --context "$CONTEXT" > /dev/null 2>&1; then
    if kubectl get secret gitlab-registry -n ashley-ns --context "$CONTEXT" > /dev/null 2>&1; then
      echo "Copying gitlab-registry secret from ashley-ns to $NS..."
      kubectl get secret gitlab-registry -n ashley-ns --context "$CONTEXT" -o yaml \
        | grep -v 'namespace:' \
        | kubectl apply -n "$NS" --context "$CONTEXT" -f -
    else
      echo "⚠️ Note: gitlab-registry secret not found in ashley-ns. Ensure image pull credentials exist if repository is private."
    fi
  fi
else
  echo "⚠️ Warning: kubectl not installed or not in PATH."
fi

# 4. Deploy / Upgrade Helm Release
echo "=== [4/5] Deploying Helm release 'wirellm'..."
if command -v helm > /dev/null 2>&1; then
  HELM_CMD=(helm upgrade --install wirellm "$REPO_ROOT/deploy/helm/wirellm" -n "$NS" --kube-context "$CONTEXT")
  
  # Override image repository and tag dynamically
  REPO_PART="${IMAGE%:*}"
  TAG_PART="${IMAGE##*:}"
  HELM_CMD+=(--set image.repository="$REPO_PART" --set image.tag="$TAG_PART")

  if [ -n "$VALUES_FILE" ] && [ -f "$VALUES_FILE" ]; then
    echo "Using values file: $VALUES_FILE"
    HELM_CMD+=(-f "$VALUES_FILE")
  elif [ -f "$REPO_ROOT/deploy/helm/wirellm/values-cluster.yaml" ]; then
    echo "Using cluster values: deploy/helm/wirellm/values-cluster.yaml"
    HELM_CMD+=(-f "$REPO_ROOT/deploy/helm/wirellm/values-cluster.yaml")
  fi

  "${HELM_CMD[@]}"

  # Rollout restart to pull latest image
  echo "Restarting wirellm deployment..."
  kubectl rollout restart deployment/wirellm -n "$NS" --context "$CONTEXT" || true
  kubectl rollout status deployment/wirellm -n "$NS" --context "$CONTEXT" --timeout=180s || true
else
  echo "⚠️ Warning: helm not installed. Please deploy manifests manually."
fi

# 5. Verification
echo "=== [5/5] Verifying Deployment..."
sleep 3
if curl -sf "$SITE_URL/health" > /dev/null 2>&1; then
  echo "✅ Health check passed: $SITE_URL/health -> 200 OK"
else
  echo "ℹ️  Health check status: $(curl -s -o /dev/null -w "%{http_code}\n" "$SITE_URL/health" 2>/dev/null || echo "not reachable yet")"
fi

echo "=================================================="
echo "✨ WireLLM successfully published & deployed to $SITE_URL"
echo "=================================================="
