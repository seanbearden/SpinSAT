#!/usr/bin/env python3
"""
Cloud-distributed Optuna tuning for SpinSAT.

Orchestrates:
  1. Cloud SQL PostgreSQL instance (shared Optuna storage)
  2. N spot VMs as independent Optuna workers
  3. GCS bucket for solver binary + CNF instances

Each worker runs optuna_tune.py --db-url pointing at the shared Cloud SQL.
Optuna's heartbeat mechanism handles preemption: if a worker dies mid-trial,
the trial is marked FAIL after grace_period and retried by another worker.

Usage (via optuna_tune.py):
    python3 scripts/optuna_tune.py --campaign campaigns/tune_ode_full.yaml --cloud
    python3 scripts/optuna_tune.py --campaign campaigns/tune_ode_full.yaml --cloud --cloud-status
    python3 scripts/optuna_tune.py --campaign campaigns/tune_ode_full.yaml --cloud --cloud-cleanup
"""

import json
import os
import subprocess
import sys
import tempfile
import time
from datetime import datetime
from pathlib import Path

PROJECT_ROOT = Path(__file__).parent.parent


class CloudOptunaError(Exception):
    pass


class CloudOptuna:
    """Manages distributed Optuna tuning on GCP."""

    def __init__(
        self,
        campaign_path,
        config,
        n_workers=4,
        zone="us-central1-a",
        machine_type="c3-standard-4",
        max_hours=12,
        project="spinsat",
        bucket="spinsat-benchmarks",
        db_instance="spinsat-optuna",
        db_region="us-central1",
    ):
        self.campaign_path = campaign_path
        self.config = config
        self.n_workers = n_workers
        self.zone = zone
        self.machine_type = machine_type
        self.max_hours = max_hours
        self.project = project
        self.bucket = bucket
        self.db_instance = db_instance
        self.db_region = db_region

        self.db_name = "optuna"
        self.db_user = "optuna"
        self.db_password = None  # Generated or fetched

        ts = datetime.now().strftime("%Y%m%d-%H%M%S")
        self.worker_prefix = f"spinsat-optuna-{ts}"

    # ------------------------------------------------------------------
    # GCP helpers
    # ------------------------------------------------------------------

    def _gcloud(self, args, check=True, timeout=120):
        cmd = ["gcloud"] + args + ["--project", self.project]
        result = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout)
        if check and result.returncode != 0:
            raise CloudOptunaError(f"gcloud failed: {' '.join(args)}\n{result.stderr}")
        return result

    def _gsutil(self, args, check=True, timeout=300):
        cmd = ["gsutil"] + args
        result = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout)
        if check and result.returncode != 0:
            raise CloudOptunaError(f"gsutil failed: {' '.join(args)}\n{result.stderr}")
        return result

    # ------------------------------------------------------------------
    # Cloud SQL lifecycle
    # ------------------------------------------------------------------

    def _get_db_connection_name(self):
        """Get the Cloud SQL instance connection name."""
        result = self._gcloud([
            "sql", "instances", "describe", self.db_instance,
            "--format", "value(connectionName)",
        ], check=False)
        if result.returncode == 0 and result.stdout.strip():
            return result.stdout.strip()
        return None

    def _get_db_ip(self):
        """Get the Cloud SQL instance public IP."""
        result = self._gcloud([
            "sql", "instances", "describe", self.db_instance,
            "--format", "value(ipAddresses[0].ipAddress)",
        ], check=False)
        if result.returncode == 0 and result.stdout.strip():
            return result.stdout.strip()
        return None

    def _ensure_cloud_sql(self):
        """Create Cloud SQL PostgreSQL instance if it doesn't exist.

        Always ensures user and database exist (idempotent).
        """
        conn_name = self._get_db_connection_name()
        if conn_name:
            print(f"  Cloud SQL instance exists: {conn_name}")
            ip = self._get_db_ip()
            print(f"  Public IP: {ip}")
        else:
            print(f"  Creating Cloud SQL instance: {self.db_instance}...")
            self._gcloud([
                "sql", "instances", "create", self.db_instance,
                "--database-version", "POSTGRES_15",
                "--tier", "db-g1-small",
                "--region", self.db_region,
                "--assign-ip",
                "--authorized-networks", "0.0.0.0/0",  # Workers need access
                "--storage-size", "10GB",
                "--storage-type", "SSD",
                "--no-backup",
                "--database-flags", "max_connections=100",
            ], timeout=900)  # Cloud SQL creation can take 10-15 minutes
            ip = self._get_db_ip()
            print(f"  Cloud SQL created: {ip}")

        # Always ensure user and database exist (idempotent — ignores "already exists")
        print(f"  Ensuring DB user '{self.db_user}' and database '{self.db_name}'...")
        self._gcloud([
            "sql", "users", "create", self.db_user,
            "--instance", self.db_instance,
            "--password", self.db_password,
        ], check=False)  # Ignore if user already exists
        self._gcloud([
            "sql", "databases", "create", self.db_name,
            "--instance", self.db_instance,
        ], check=False)  # Ignore if database already exists

        print(f"  Cloud SQL ready: {ip}")
        return ip

    def _generate_password(self):
        """Generate a random password."""
        import secrets
        return secrets.token_urlsafe(24)

    def _get_or_create_password(self):
        """Get or create password, stored in a local dotfile.

        Avoids requiring Secret Manager API. The password file is
        gitignored and lives alongside the optuna studies.
        """
        pw_file = PROJECT_ROOT / "optuna_studies" / f".db-password-{self.db_instance}"
        pw_file.parent.mkdir(parents=True, exist_ok=True)

        if pw_file.exists():
            password = pw_file.read_text().strip()
            if password:
                return password

        password = self._generate_password()
        pw_file.write_text(password)
        pw_file.chmod(0o600)
        print(f"  Generated DB password → {pw_file}")
        return password

    def _build_db_url(self, db_ip):
        """Build PostgreSQL connection URL."""
        return f"postgresql://{self.db_user}:{self.db_password}@{db_ip}:5432/{self.db_name}"

    # ------------------------------------------------------------------
    # GCS setup
    # ------------------------------------------------------------------

    def _ensure_bucket(self):
        """Create GCS bucket if it doesn't exist."""
        result = self._gsutil(["ls", f"gs://{self.bucket}"], check=False)
        if result.returncode == 0:
            print(f"  GCS bucket exists: gs://{self.bucket}")
            return

        print(f"  Creating GCS bucket: gs://{self.bucket}...")
        self._gsutil(["mb", "-l", self.db_region, f"gs://{self.bucket}"])

    def _upload_solver(self):
        """Upload musl solver binary to GCS."""
        musl_path = PROJECT_ROOT / "target" / "x86_64-unknown-linux-musl" / "release" / "spinsat"
        if not musl_path.exists():
            print("  Building musl binary...")
            result = subprocess.run(
                ["cargo", "build", "--release", "--target", "x86_64-unknown-linux-musl"],
                cwd=str(PROJECT_ROOT), capture_output=True, text=True, timeout=300,
            )
            if result.returncode != 0:
                raise CloudOptunaError(f"Failed to build musl binary:\n{result.stderr}")

        if not musl_path.exists():
            raise CloudOptunaError(
                "musl binary not found. Install: rustup target add x86_64-unknown-linux-musl"
            )

        gcs_path = f"gs://{self.bucket}/optuna/spinsat"
        print(f"  Uploading solver to {gcs_path}...")
        self._gsutil(["cp", str(musl_path), gcs_path])

    def _upload_instances(self):
        """Upload CNF instances to GCS."""
        instances = self.config.resolved_instances
        gcs_dir = f"gs://{self.bucket}/optuna/instances/"

        # Check which already exist
        result = self._gsutil(["ls", gcs_dir], check=False)
        existing = set()
        if result.returncode == 0:
            existing = {os.path.basename(p.strip()) for p in result.stdout.strip().split("\n") if p.strip()}

        to_upload = [i for i in instances if os.path.basename(i) not in existing]
        if not to_upload:
            print(f"  All {len(instances)} instances already in GCS")
            return

        print(f"  Uploading {len(to_upload)} instances to GCS ({len(existing)} already there)...")
        # Upload in parallel with gsutil -m
        with tempfile.NamedTemporaryFile(mode="w", suffix=".txt", delete=False) as f:
            for path in to_upload:
                f.write(f"{path}\n")
            filelist = f.name

        try:
            self._gsutil(["-m", "cp", "-I", gcs_dir], check=True, timeout=600)
        except CloudOptunaError:
            # Fall back to sequential
            for path in to_upload:
                self._gsutil(["cp", path, gcs_dir], timeout=120)
        finally:
            os.unlink(filelist)

    def _upload_campaign(self):
        """Upload campaign YAML to GCS."""
        gcs_path = f"gs://{self.bucket}/optuna/campaign.yaml"
        print(f"  Uploading campaign config...")
        self._gsutil(["cp", self.campaign_path, gcs_path])

    # ------------------------------------------------------------------
    # Worker VMs
    # ------------------------------------------------------------------

    def _create_worker_startup_script(self, db_url, worker_id):
        """Generate the startup script for a worker VM."""
        gcs_solver = f"gs://{self.bucket}/optuna/spinsat"
        gcs_instances = f"gs://{self.bucket}/optuna/instances/"
        gcs_campaign = f"gs://{self.bucket}/optuna/campaign.yaml"
        n_trials = self.config.n_trials

        script = f"""#!/bin/bash
set -uo pipefail
# Note: no 'set -e' — the retry loop needs to handle non-zero exits

# Safety: auto-shutdown after max hours
shutdown -h +{self.max_hours * 60} "SpinSAT Optuna worker safety timeout"

echo "=== SpinSAT Optuna Worker {worker_id} ==="
echo "Started: $(date -u)"

# Install Python and dependencies
if command -v dnf &>/dev/null; then
    dnf install -y python3 python3-pip postgresql 2>/dev/null || true
elif command -v apt-get &>/dev/null; then
    apt-get update -qq && apt-get install -y python3 python3-pip python3-venv postgresql-client 2>/dev/null || true
fi

# Create venv and install optuna + psycopg2
python3 -m venv /opt/optuna-env
source /opt/optuna-env/bin/activate
pip install --quiet optuna psycopg2-binary pyyaml

# Download solver binary
mkdir -p /opt/spinsat
gsutil cp {gcs_solver} /opt/spinsat/spinsat
chmod +x /opt/spinsat/spinsat
echo "Solver version: $(/opt/spinsat/spinsat -V 2>&1 || true)"

# Download instances
mkdir -p /opt/spinsat/instances
gsutil -m cp "{gcs_instances}*" /opt/spinsat/instances/
echo "Instances: $(ls /opt/spinsat/instances/*.cnf 2>/dev/null | wc -l)"

# Download campaign config + tuning scripts
gsutil cp {gcs_campaign} /opt/spinsat/campaign.yaml
gsutil cp "gs://{self.bucket}/optuna/scripts/optuna_tune.py" /opt/spinsat/optuna_tune.py
gsutil cp "gs://{self.bucket}/optuna/scripts/campaign_config.py" /opt/spinsat/campaign_config.py
gsutil cp "gs://{self.bucket}/optuna/scripts/benchmark_suite.py" /opt/spinsat/benchmark_suite.py

# Patch campaign to use local instance paths (replace entire patterns block)
python3 -c "
import yaml, sys
with open('/opt/spinsat/campaign.yaml') as f:
    cfg = yaml.safe_load(f)
cfg['instances']['patterns'] = ['/opt/spinsat/instances/*.cnf']
with open('/opt/spinsat/campaign.yaml', 'w') as f:
    yaml.dump(cfg, f, default_flow_style=False)
print('Patched campaign.yaml instances to /opt/spinsat/instances/*.cnf')
"

# Patch SOLVER_CMD in optuna_tune.py to use downloaded binary
sed -i 's|^SOLVER_CMD = .*|SOLVER_CMD = "/opt/spinsat/spinsat"|' /opt/spinsat/optuna_tune.py

# Wait for Cloud SQL to be reachable
echo "Checking DB connectivity..."
for i in $(seq 1 30); do
    if python3 -c "
import psycopg2
conn = psycopg2.connect('{db_url}')
conn.close()
print('DB connected')
" 2>/dev/null; then
        break
    fi
    echo "  Waiting for DB... (attempt $i/30)"
    sleep 10
done

# Run Optuna worker with crash recovery loop
echo "Starting Optuna worker {worker_id}..."
cd /opt/spinsat
MAX_RETRIES=5
for attempt in $(seq 1 $MAX_RETRIES); do
    echo "=== Attempt $attempt/$MAX_RETRIES ($(date -u)) ===" >> /var/log/optuna-worker.log
    python3 optuna_tune.py \\
        --campaign campaign.yaml \\
        --db-url "{db_url}" \\
        --worker-id "{worker_id}" \\
        --n-trials {n_trials} \\
        >> /var/log/optuna-worker.log 2>&1
    exit_code=$?
    if [ $exit_code -eq 0 ]; then
        echo "Worker completed successfully" >> /var/log/optuna-worker.log
        break
    fi
    echo "Worker crashed (exit=$exit_code), retrying in 30s..." >> /var/log/optuna-worker.log
    sleep 30
done

echo "Worker {worker_id} finished: $(date -u)"

# Upload log to GCS
gsutil cp /var/log/optuna-worker.log "gs://{self.bucket}/optuna/logs/{worker_id}.log" 2>/dev/null || true

# Self-terminate
shutdown -h now "Worker complete"
"""
        return script

    def _create_workers(self, db_url):
        """Create N spot VM workers."""
        print(f"\nCreating {self.n_workers} spot VM workers...")

        # Upload scripts to GCS so workers can download them
        scripts_dir = Path(__file__).parent
        gcs_scripts = f"gs://{self.bucket}/optuna/scripts/"
        for script_name in ["optuna_tune.py", "campaign_config.py", "benchmark_suite.py"]:
            self._gsutil(["cp", str(scripts_dir / script_name), gcs_scripts])

        worker_names = []
        for i in range(self.n_workers):
            worker_id = f"{self.worker_prefix}-w{i}"
            worker_names.append(worker_id)

            startup_script = self._create_worker_startup_script(db_url, worker_id)

            # Write startup script to temp file
            with tempfile.NamedTemporaryFile(
                mode="w", suffix=".sh", delete=False, prefix=f"optuna_worker_{i}_"
            ) as f:
                f.write(startup_script)
                script_path = f.name

            try:
                self._gcloud([
                    "compute", "instances", "create", worker_id,
                    "--zone", self.zone,
                    "--machine-type", self.machine_type,
                    "--provisioning-model=SPOT",
                    "--instance-termination-action", "STOP",
                    "--restart-on-failure",
                    "--image-family", "debian-12",
                    "--image-project", "debian-cloud",
                    "--boot-disk-size", "30GB",
                    "--boot-disk-type", "pd-ssd",
                    "--scopes", "storage-ro,sql-admin",
                    "--metadata-from-file", f"startup-script={script_path}",
                ], timeout=180)
                print(f"  Created: {worker_id}")
            except CloudOptunaError as e:
                print(f"  Failed to create {worker_id}: {e}", file=sys.stderr)
            finally:
                os.unlink(script_path)

        return worker_names

    # ------------------------------------------------------------------
    # Orchestration
    # ------------------------------------------------------------------

    def run(self):
        """Run the full distributed tuning pipeline."""
        print("=" * 60)
        print("DISTRIBUTED OPTUNA TUNING")
        print("=" * 60)
        print(f"  Study: {self.config.study_name}")
        print(f"  Workers: {self.n_workers} × {self.machine_type} (spot)")
        print(f"  Zone: {self.zone}")
        print(f"  Trials: {self.config.n_trials}")
        print(f"  Instances: {len(self.config.resolved_instances)}")
        print(f"  Seeds: {self.config.seeds}")
        print(f"  Timeout: {self.config.timeout_s}s per instance")
        print(f"  Max VM lifetime: {self.max_hours}h")
        print()

        # Step 1: Cloud SQL
        print("[1/5] Cloud SQL PostgreSQL...")
        self.db_password = self._get_or_create_password()
        db_ip = self._ensure_cloud_sql()
        db_url = self._build_db_url(db_ip)
        print()

        # Step 2: GCS bucket + uploads
        print("[2/5] GCS bucket and uploads...")
        self._ensure_bucket()
        self._upload_solver()
        self._upload_instances()
        self._upload_campaign()
        print()

        # Step 3: Create workers
        print("[3/5] Creating worker VMs...")
        worker_names = self._create_workers(db_url)
        print()

        # Step 4: Monitor
        print("[4/5] Workers launched. Monitoring...")
        print(f"  Study DB: {self._mask_url(db_url)}")
        print(f"  Workers: {', '.join(worker_names)}")
        print()
        print("  Monitor locally:")
        print(f"    python3 scripts/optuna_tune.py --campaign {self.campaign_path} "
              f"--db-url '{self._mask_url(db_url)}' --validate-best")
        print()
        print("  Or check status:")
        print(f"    python3 scripts/optuna_tune.py --campaign {self.campaign_path} "
              f"--cloud --cloud-status")
        print()
        print("  Worker logs stream to GCS:")
        print(f"    gsutil cat gs://{self.bucket}/optuna/logs/<worker-id>.log")
        print()

        # Step 5: Poll until done
        self._poll_study(db_url, worker_names)

    def _poll_study(self, db_url, worker_names):
        """Poll study progress until all trials complete or workers die."""
        try:
            import optuna
        except ImportError:
            print("  (optuna not installed locally — cannot poll. Check status manually.)")
            return

        print("Polling study progress (Ctrl+C to detach — workers continue)...")
        print()

        poll_interval = 60  # seconds
        last_complete = -1

        try:
            while True:
                try:
                    storage = optuna.storages.RDBStorage(url=db_url)
                    study = optuna.load_study(
                        study_name=self.config.study_name, storage=storage
                    )

                    n_complete = len([t for t in study.trials
                                      if t.state == optuna.trial.TrialState.COMPLETE])
                    n_pruned = len([t for t in study.trials
                                    if t.state == optuna.trial.TrialState.PRUNED])
                    n_running = len([t for t in study.trials
                                     if t.state == optuna.trial.TrialState.RUNNING])
                    n_fail = len([t for t in study.trials
                                   if t.state == optuna.trial.TrialState.FAIL])

                    total_done = n_complete + n_pruned
                    best_val = study.best_value if n_complete > 0 else float("inf")

                    if total_done != last_complete:
                        ts = datetime.now().strftime("%H:%M:%S")
                        print(
                            f"  [{ts}] {total_done}/{self.config.n_trials} done "
                            f"({n_complete} complete, {n_pruned} pruned, "
                            f"{n_running} running, {n_fail} failed) | "
                            f"Best PAR-2: {best_val:.2f}"
                        )
                        last_complete = total_done

                    if total_done >= self.config.n_trials:
                        print("\n  All trials complete!")
                        self._print_final_report(study)
                        break

                    # Check if any workers still alive
                    alive = self._count_alive_workers(worker_names)
                    if alive == 0 and n_running == 0:
                        print(f"\n  No workers alive and no running trials. "
                              f"Completed {total_done}/{self.config.n_trials} trials.")
                        if n_complete > 0:
                            self._print_final_report(study)
                        break

                except Exception as e:
                    print(f"  Poll error: {e}", file=sys.stderr)

                time.sleep(poll_interval)

        except KeyboardInterrupt:
            print("\n  Detached from monitoring. Workers continue on GCP.")
            print(f"  Re-attach: python3 scripts/optuna_tune.py "
                  f"--campaign {self.campaign_path} --cloud --cloud-status")

    def _count_alive_workers(self, worker_names):
        """Count how many worker VMs are still running."""
        try:
            result = self._gcloud([
                "compute", "instances", "list",
                "--filter", f"name~{self.worker_prefix}",
                "--format", "value(name,status)",
            ], check=False)
            if result.returncode != 0:
                return -1  # Unknown

            alive = 0
            for line in result.stdout.strip().split("\n"):
                if line.strip():
                    parts = line.split()
                    if len(parts) >= 2 and parts[1] in ("RUNNING", "STAGING", "PROVISIONING"):
                        alive += 1
            return alive
        except Exception:
            return -1

    def _print_final_report(self, study):
        """Print final study results."""
        best = study.best_trial
        print(f"\n  Best trial #{best.number}: PAR-2 = {best.value:.2f}")
        print("  Parameters:")
        for k, v in sorted(best.params.items()):
            if isinstance(v, float):
                print(f"    {k}: {v:.6g}")
            else:
                print(f"    {k}: {v}")

    def _mask_url(self, url):
        """Mask password in DB URL for display."""
        if "@" not in url:
            return url
        pre, post = url.split("@", 1)
        if ":" in pre:
            scheme_user = pre.rsplit(":", 1)[0]
            return f"{scheme_user}:***@{post}"
        return url

    # ------------------------------------------------------------------
    # Status & cleanup
    # ------------------------------------------------------------------

    def status(self):
        """Check status of cloud resources and study progress."""
        print("=== CLOUD OPTUNA STATUS ===\n")

        # Cloud SQL
        conn_name = self._get_db_connection_name()
        if conn_name:
            ip = self._get_db_ip()
            print(f"Cloud SQL: {self.db_instance} ({ip})")
        else:
            print(f"Cloud SQL: {self.db_instance} (not found)")

        # Workers
        result = self._gcloud([
            "compute", "instances", "list",
            "--filter", "name~spinsat-optuna",
            "--format", "table(name,zone,machineType.basename(),status,creationTimestamp)",
        ], check=False)
        if result.stdout.strip():
            print(f"\nWorker VMs:")
            print(result.stdout)
        else:
            print("\nNo worker VMs found.")

        # Study progress (if we can connect)
        if conn_name:
            try:
                self.db_password = self._get_or_create_password()
                db_url = self._build_db_url(self._get_db_ip())

                import optuna
                storage = optuna.storages.RDBStorage(url=db_url)
                study = optuna.load_study(
                    study_name=self.config.study_name, storage=storage
                )

                n_complete = len([t for t in study.trials
                                  if t.state == optuna.trial.TrialState.COMPLETE])
                n_pruned = len([t for t in study.trials
                                if t.state == optuna.trial.TrialState.PRUNED])
                n_running = len([t for t in study.trials
                                 if t.state == optuna.trial.TrialState.RUNNING])
                n_fail = len([t for t in study.trials
                               if t.state == optuna.trial.TrialState.FAIL])

                print(f"\nStudy: {self.config.study_name}")
                print(f"  Complete: {n_complete}")
                print(f"  Pruned:   {n_pruned}")
                print(f"  Running:  {n_running}")
                print(f"  Failed:   {n_fail}")
                print(f"  Total:    {n_complete + n_pruned + n_running + n_fail}/{self.config.n_trials}")

                if n_complete > 0:
                    self._print_final_report(study)
            except Exception as e:
                print(f"\nCould not connect to study: {e}")

    def cleanup(self):
        """Delete cloud resources."""
        print("=== CLOUD OPTUNA CLEANUP ===\n")

        # Delete workers
        result = self._gcloud([
            "compute", "instances", "list",
            "--filter", "name~spinsat-optuna",
            "--format", "value(name,zone)",
        ], check=False)

        if result.stdout.strip():
            for line in result.stdout.strip().split("\n"):
                parts = line.split()
                if len(parts) >= 2:
                    name, zone = parts[0], parts[1]
                    print(f"  Deleting VM: {name}...")
                    self._gcloud([
                        "compute", "instances", "delete", name,
                        "--zone", zone, "--quiet",
                    ], check=False, timeout=60)
        else:
            print("  No worker VMs to delete.")

        # Don't auto-delete Cloud SQL — it has study data
        print(f"\n  Cloud SQL instance '{self.db_instance}' preserved (contains study data).")
        print(f"  To delete: gcloud sql instances delete {self.db_instance} --project {self.project}")

        # Don't auto-delete GCS — it has solver + instances
        print(f"  GCS bucket '{self.bucket}' preserved.")
        print(f"  To clean: gsutil -m rm gs://{self.bucket}/optuna/**")
