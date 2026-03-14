#!/usr/bin/env bash
set -euo pipefail

# Nectar Edge Runtime — deploy to 29 GCP Cloud Run regions
# Usage: ./edge/deploy.sh [--build-only]

PROJECT="nectar-edge"
REPO="us-central1-docker.pkg.dev/${PROJECT}/nectar-runtime"
IMAGE="${REPO}/edge:latest"
SERVICE="nectar-edge"
MEMORY="512Mi"
CPU="1"
MAX_INSTANCES="10"
MIN_INSTANCES="0"  # scale-to-zero

REGIONS=(
  # US (8)
  us-central1 us-east1 us-east4 us-west1 us-west2 us-west4 us-south1 northamerica-northeast1
  # Europe (8)
  europe-west1 europe-west2 europe-west3 europe-west4 europe-west6 europe-west9 europe-north1 europe-southwest1
  # Asia-Pacific (9)
  asia-east1 asia-east2 asia-northeast1 asia-northeast2 asia-northeast3 asia-south1 asia-south2 asia-southeast1 australia-southeast1
  # Latin America / Middle East / Africa (4)
  southamerica-east1 southamerica-west1 me-central1 africa-south1
)

echo "=== Building Nectar edge runtime ==="
docker build -t "${IMAGE}" -f edge/Dockerfile .

echo "=== Pushing to Artifact Registry ==="
docker push "${IMAGE}"

if [[ "${1:-}" == "--build-only" ]]; then
  echo "Build complete. Skipping deploy."
  exit 0
fi

echo "=== Deploying to ${#REGIONS[@]} regions ==="
for region in "${REGIONS[@]}"; do
  echo "  Deploying to ${region}..."
  gcloud run deploy "${SERVICE}" \
    --project="${PROJECT}" \
    --region="${region}" \
    --image="${IMAGE}" \
    --platform=managed \
    --memory="${MEMORY}" \
    --cpu="${CPU}" \
    --max-instances="${MAX_INSTANCES}" \
    --min-instances="${MIN_INSTANCES}" \
    --allow-unauthenticated \
    --port=8080 \
    --set-env-vars="NECTAR_REGION=${region}" \
    --quiet &
done

echo "Waiting for all deployments..."
wait
echo "=== Done. ${#REGIONS[@]} regions deployed. ==="

# Print service URLs
echo ""
echo "=== Service URLs ==="
for region in "${REGIONS[@]}"; do
  url=$(gcloud run services describe "${SERVICE}" --project="${PROJECT}" --region="${region}" --format="value(status.url)" 2>/dev/null || echo "pending")
  echo "  ${region}: ${url}"
done
