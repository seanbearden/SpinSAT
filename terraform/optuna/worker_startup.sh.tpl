#!/bin/bash
# SpinSAT Optuna worker startup — pre-baked image, fast boot
# Only downloads solver + instances + scripts, then runs.
set -uo pipefail

# Safety: auto-shutdown after max hours
shutdown -h +$((${max_hours} * 60)) "SpinSAT Optuna worker safety timeout"

WORKER_ID=$(hostname)
echo "=== SpinSAT Optuna Worker $WORKER_ID ==="
echo "Started: $(date -u)"

# Activate pre-baked venv
source /opt/optuna-env/bin/activate

# Download solver binary
gsutil cp "gs://${gcs_bucket}/optuna/spinsat" /opt/spinsat/spinsat
chmod +x /opt/spinsat/spinsat
echo "Solver: $(/opt/spinsat/spinsat -V 2>&1 || true)"

# Download instances
gsutil -m cp "gs://${gcs_bucket}/optuna/instances/*" /opt/spinsat/instances/
echo "Instances: $(ls /opt/spinsat/instances/*.cnf 2>/dev/null | wc -l)"

# Download campaign + scripts
gsutil cp "gs://${gcs_bucket}/optuna/${campaign_yaml}" /opt/spinsat/campaign.yaml
gsutil cp "gs://${gcs_bucket}/optuna/scripts/optuna_tune.py" /opt/spinsat/optuna_tune.py
gsutil cp "gs://${gcs_bucket}/optuna/scripts/campaign_config.py" /opt/spinsat/campaign_config.py
gsutil cp "gs://${gcs_bucket}/optuna/scripts/benchmark_suite.py" /opt/spinsat/benchmark_suite.py

# Patch paths for this VM
sed -i 's|patterns:.*|patterns: ["/opt/spinsat/instances/*.cnf"]|' /opt/spinsat/campaign.yaml
sed -i 's|SOLVER_CMD = .*|SOLVER_CMD = "/opt/spinsat/spinsat"|' /opt/spinsat/optuna_tune.py

# Wait for Cloud SQL
echo "Checking DB connectivity..."
for i in $(seq 1 30); do
    if python3 -c "
import psycopg2
conn = psycopg2.connect('${db_url}')
conn.close()
print('DB connected')
" 2>/dev/null; then
        break
    fi
    echo "  Waiting for DB... (attempt $i/30)"
    sleep 10
done

# Run Optuna worker with crash recovery loop
cd /opt/spinsat
MAX_RETRIES=10
for attempt in $(seq 1 $MAX_RETRIES); do
    echo "=== Attempt $attempt/$MAX_RETRIES ($(date -u)) ===" >> /var/log/optuna-worker.log
    python3 optuna_tune.py \
        --campaign campaign.yaml \
        --db-url "${db_url}" \
        --worker-id "$WORKER_ID" \
        --n-trials ${n_trials} \
        >> /var/log/optuna-worker.log 2>&1
    exit_code=$?
    if [ $exit_code -eq 0 ]; then
        echo "Worker completed successfully" >> /var/log/optuna-worker.log
        break
    fi
    echo "Worker crashed (exit=$exit_code), retrying in 30s..." >> /var/log/optuna-worker.log
    sleep 30
done

echo "Worker $WORKER_ID finished: $(date -u)"

# Upload log to GCS
gsutil cp /var/log/optuna-worker.log "gs://${gcs_bucket}/optuna/logs/$WORKER_ID.log" 2>/dev/null || true

# Self-terminate
shutdown -h now "Worker complete"
