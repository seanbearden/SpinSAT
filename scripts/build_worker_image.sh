#!/bin/bash
# Build a pre-baked GCE image for Optuna workers.
# Installs Python, optuna, psycopg2 so workers boot in ~30s instead of ~4min.
#
# Usage: ./scripts/build_worker_image.sh [--project spinsat] [--zone us-central1-a]
set -euo pipefail

PROJECT="${1:-spinsat}"
ZONE="${2:-us-central1-a}"
IMAGE_NAME="spinsat-optuna-worker"
BUILDER_NAME="spinsat-image-builder"

echo "=== Building SpinSAT Optuna worker image ==="
echo "  Project: $PROJECT"
echo "  Zone: $ZONE"
echo "  Image: $IMAGE_NAME"

# Check if image already exists
if gcloud compute images describe "$IMAGE_NAME" --project="$PROJECT" &>/dev/null; then
    echo "  Image '$IMAGE_NAME' already exists."
    read -p "  Delete and rebuild? [y/N] " -n 1 -r
    echo
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        gcloud compute images delete "$IMAGE_NAME" --project="$PROJECT" --quiet
    else
        echo "  Keeping existing image."
        exit 0
    fi
fi

# Clean up any leftover builder VM
gcloud compute instances delete "$BUILDER_NAME" --zone="$ZONE" --project="$PROJECT" --quiet 2>/dev/null || true

# Create builder VM
echo "[1/5] Creating builder VM..."
gcloud compute instances create "$BUILDER_NAME" \
    --zone="$ZONE" \
    --machine-type=e2-medium \
    --image-family=debian-12 \
    --image-project=debian-cloud \
    --boot-disk-size=20GB \
    --boot-disk-type=pd-ssd \
    --project="$PROJECT"

# Wait for SSH
echo "[2/5] Waiting for SSH..."
for i in $(seq 1 30); do
    if gcloud compute ssh "$BUILDER_NAME" --zone="$ZONE" --project="$PROJECT" \
        --command="echo ok" --quiet 2>/dev/null; then
        break
    fi
    sleep 5
done

# Install dependencies
echo "[3/5] Installing Python + Optuna..."
gcloud compute ssh "$BUILDER_NAME" --zone="$ZONE" --project="$PROJECT" --quiet --command="
    sudo apt-get update -qq
    sudo apt-get install -y python3 python3-pip python3-venv
    sudo python3 -m venv /opt/optuna-env
    sudo /opt/optuna-env/bin/pip install --quiet optuna psycopg2-binary pyyaml
    sudo mkdir -p /opt/spinsat/instances
    sudo touch /opt/spinsat/.image-ready
    echo 'Dependencies installed'
"

# Stop VM (required before creating image)
echo "[4/5] Stopping builder VM..."
gcloud compute instances stop "$BUILDER_NAME" --zone="$ZONE" --project="$PROJECT"

# Wait for stopped state
while true; do
    status=$(gcloud compute instances describe "$BUILDER_NAME" --zone="$ZONE" --project="$PROJECT" --format='value(status)')
    if [ "$status" = "TERMINATED" ]; then break; fi
    sleep 5
done

# Create image from disk
echo "[5/5] Creating image..."
gcloud compute images create "$IMAGE_NAME" \
    --source-disk="$BUILDER_NAME" \
    --source-disk-zone="$ZONE" \
    --project="$PROJECT" \
    --description="Pre-baked: Debian 12 + Python venv + optuna + psycopg2" \
    --family=spinsat-optuna

# Clean up builder
gcloud compute instances delete "$BUILDER_NAME" --zone="$ZONE" --project="$PROJECT" --quiet

echo ""
echo "=== Image ready: $IMAGE_NAME ==="
echo "  Use in Terraform: google_compute_image.optuna_worker"
echo "  Use in gcloud:    --image=$IMAGE_NAME --image-project=$PROJECT"
