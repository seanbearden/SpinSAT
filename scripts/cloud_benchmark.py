#!/usr/bin/env python3
"""
GCP Cloud Benchmark Module for SpinSAT.

Manages the lifecycle of a GCP Compute Engine instance for running benchmarks
under competition-faithful conditions (8-way parallel, core-pinned, turbo off).

Used by benchmark_suite.py --cloud. Not intended to be run standalone.
"""

import json
import os
import shutil
import subprocess
import sys
import tarfile
import tempfile
import time
from datetime import datetime
from pathlib import Path

PROJECT_ROOT = Path(__file__).parent.parent
CLOUD_WORKER = Path(__file__).parent / "cloud_worker.sh"

# GCP defaults
GCP_PROJECT = "spinsat"
DEFAULT_ZONE = "us-central1-a"
DEFAULT_MACHINE = "n2-highcpu-8"
DEFAULT_IMAGE_FAMILY = "rocky-linux-9-optimized-gcp"
DEFAULT_IMAGE_PROJECT = "rocky-linux-cloud"
DEFAULT_MAX_HOURS = 6
DEFAULT_PARALLELISM = 8
DEFAULT_BUCKET = "spinsat-benchmarks"
DEFAULT_RESULTS_BUCKET = "spinsat-results"


class CloudBenchmarkError(Exception):
    pass


class CloudBenchmark:
    """Manages a GCP instance for running SpinSAT benchmarks."""

    def __init__(
        self,
        zone=DEFAULT_ZONE,
        machine_type=DEFAULT_MACHINE,
        spot=True,
        max_hours=DEFAULT_MAX_HOURS,
        parallelism=DEFAULT_PARALLELISM,
        bucket=None,
        results_bucket=DEFAULT_RESULTS_BUCKET,
        project=GCP_PROJECT,
        actor=None,
    ):
        self.zone = zone
        self.machine_type = machine_type
        self.spot = spot
        self.max_hours = max_hours
        self.parallelism = parallelism
        self.bucket = bucket
        self.results_bucket = results_bucket
        self.project = project

        ts = datetime.now().strftime("%Y%m%d-%H%M%S")
        self.instance_name = f"spinsat-bench-{ts}"
        self._instance_created = False
        self._run_id = ts

        # Labels for monitoring and cost attribution
        self._labels = {
            "purpose": "benchmark",
            "crew-member": (actor or "unknown").replace("/", "-"),
            "run-id": ts,
        }

    # ------------------------------------------------------------------
    # GCP helpers
    # ------------------------------------------------------------------

    def _gcloud(self, args, check=True, capture=True, timeout=120):
        """Run a gcloud command."""
        cmd = ["gcloud"] + args + ["--project", self.project]
        result = subprocess.run(
            cmd,
            capture_output=capture,
            text=True,
            timeout=timeout,
        )
        if check and result.returncode != 0:
            stderr = result.stderr if capture else ""
            raise CloudBenchmarkError(
                f"gcloud command failed: {' '.join(args)}\n{stderr}"
            )
        return result

    def _ssh(self, command, timeout=None, stream=False):
        """Run a command on the instance via SSH."""
        cmd = [
            "gcloud", "compute", "ssh", self.instance_name,
            "--zone", self.zone,
            "--project", self.project,
            "--command", command,
            "--quiet",
        ]
        if stream:
            # Stream output in real-time
            proc = subprocess.Popen(
                cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True
            )
            output_lines = []
            for line in proc.stdout:
                print(line, end="", flush=True)
                output_lines.append(line)
            proc.wait()
            if proc.returncode != 0:
                stderr = proc.stderr.read()
                # Don't fail on non-zero — solver timeouts cause this
                print(f"  (ssh exit code: {proc.returncode})", file=sys.stderr)
            return "".join(output_lines)
        else:
            kwargs = {}
            if timeout:
                kwargs["timeout"] = timeout
            result = subprocess.run(
                cmd, capture_output=True, text=True, **kwargs
            )
            return result

    def _scp_to(self, local_path, remote_path):
        """Copy a file to the instance."""
        cmd = [
            "gcloud", "compute", "scp",
            str(local_path),
            f"{self.instance_name}:{remote_path}",
            "--zone", self.zone,
            "--project", self.project,
            "--quiet",
        ]
        result = subprocess.run(cmd, capture_output=True, text=True, timeout=300)
        if result.returncode != 0:
            raise CloudBenchmarkError(f"scp upload failed: {result.stderr}")

    def _scp_from(self, remote_path, local_path):
        """Copy a file from the instance."""
        cmd = [
            "gcloud", "compute", "scp",
            f"{self.instance_name}:{remote_path}",
            str(local_path),
            "--zone", self.zone,
            "--project", self.project,
            "--quiet",
        ]
        result = subprocess.run(cmd, capture_output=True, text=True, timeout=300)
        if result.returncode != 0:
            raise CloudBenchmarkError(f"scp download failed: {result.stderr}")

    # ------------------------------------------------------------------
    # Instance lifecycle
    # ------------------------------------------------------------------

    def create_instance(self):
        """Create the GCP Compute Engine instance."""
        print(f"Creating instance: {self.instance_name}")
        print(f"  Zone: {self.zone}")
        print(f"  Machine: {self.machine_type}")
        print(f"  CPU: Intel Ice Lake (pinned)")
        print(f"  Spot: {self.spot}")
        print(f"  Max lifetime: {self.max_hours}h")

        label_str = ",".join(f"{k}={v}" for k, v in self._labels.items())
        gcs_results_uri = f"gs://{self.results_bucket}/runs/{self._run_id}"

        args = [
            "compute", "instances", "create", self.instance_name,
            "--zone", self.zone,
            "--machine-type", self.machine_type,
            "--min-cpu-platform", "Intel Ice Lake",
            "--image-family", DEFAULT_IMAGE_FAMILY,
            "--image-project", DEFAULT_IMAGE_PROJECT,
            "--boot-disk-size", "20GB",
            "--boot-disk-type", "pd-ssd",
            # Server-side hard limit — GCP stops the VM after this duration
            # regardless of startup script or SSH state
            "--max-run-duration", f"{self.max_hours * 3600}s",
            "--instance-termination-action", "STOP",
            # Startup script: uploads results to GCS before shutdown
            "--metadata", f"max_hours={self.max_hours},gcs_results_uri={gcs_results_uri}",
            "--metadata-from-file",
            f"startup-script={self._create_startup_script()}",
            "--scopes", "storage-rw,monitoring.write",
            "--labels", label_str,
        ]

        if self.spot:
            args.append("--provisioning-model=SPOT")

        self._gcloud(args, timeout=180)
        self._instance_created = True
        print(f"  Instance created: {self.instance_name}")

        # Wait for SSH to become available
        self._wait_for_ssh()

    def _create_startup_script(self):
        """Create a startup script that uploads results to GCS before shutdown."""
        gcs_results_uri = f"gs://{self.results_bucket}/runs/{self._run_id}"
        script = tempfile.NamedTemporaryFile(
            mode="w", suffix=".sh", delete=False, prefix="spinsat_startup_"
        )
        script.write(f"""#!/bin/bash
# Upload results to GCS before safety shutdown
upload_results_to_gcs() {{
    local gcs_uri="{gcs_results_uri}"
    if [ -f /tmp/results.json ]; then
        gsutil -q cp /tmp/results.json "$gcs_uri/results.json" 2>/dev/null || true
    fi
    if [ -d /tmp/spinsat_results ]; then
        gsutil -q -m rsync /tmp/spinsat_results/ "$gcs_uri/per_instance/" 2>/dev/null || true
    fi
}}

# Upload on SIGTERM (GCP max-run-duration sends SIGTERM before STOP)
trap 'upload_results_to_gcs; exit 0' SIGTERM

# Auto-shutdown safety net: upload results then halt
(
    sleep {self.max_hours * 3600}
    upload_results_to_gcs
    shutdown -h now "SpinSAT benchmark safety timeout"
) &

# Install gsutil if not present (Rocky Linux)
if ! command -v gsutil >/dev/null 2>&1; then
    yum install -y google-cloud-cli 2>/dev/null || true
fi
""")
        script.close()
        return script.name

    def _wait_for_ssh(self, max_wait=180):
        """Wait until SSH is available on the instance."""
        print("  Waiting for SSH (first connect may take a minute)...", end="", flush=True)
        start = time.time()
        while time.time() - start < max_wait:
            try:
                result = self._ssh("echo ok", timeout=30)
                if hasattr(result, "returncode") and result.returncode == 0:
                    print(" ready.")
                    return
            except subprocess.TimeoutExpired:
                pass  # SSH not ready yet, retry
            time.sleep(5)
            print(".", end="", flush=True)
        raise CloudBenchmarkError(f"SSH not available after {max_wait}s")

    def delete_instance(self):
        """Delete the instance. Safe to call multiple times."""
        if not self._instance_created:
            return
        print(f"Deleting instance: {self.instance_name}...")
        try:
            self._gcloud([
                "compute", "instances", "delete", self.instance_name,
                "--zone", self.zone,
                "--quiet",
            ], timeout=120)
            print(f"  Instance deleted.")
        except (CloudBenchmarkError, subprocess.TimeoutExpired) as e:
            print(f"  Warning: could not delete instance: {e}")
            print(f"  Manually delete with: gcloud compute instances delete {self.instance_name} --zone {self.zone} --project {self.project}")
        self._instance_created = False

    # ------------------------------------------------------------------
    # File transfer
    # ------------------------------------------------------------------

    def find_solver_binary(self):
        """Find the musl-compiled solver binary."""
        musl_path = PROJECT_ROOT / "target" / "x86_64-unknown-linux-musl" / "release" / "spinsat"
        if musl_path.exists():
            return musl_path

        print("Warning: musl binary not found. Building...")
        result = subprocess.run(
            ["cargo", "build", "--release", "--target", "x86_64-unknown-linux-musl"],
            cwd=str(PROJECT_ROOT),
            capture_output=True,
            text=True,
            timeout=300,
        )
        if result.returncode != 0:
            raise CloudBenchmarkError(f"Failed to build musl binary:\n{result.stderr}")

        if musl_path.exists():
            return musl_path

        raise CloudBenchmarkError(
            "musl binary not found. Install target: rustup target add x86_64-unknown-linux-musl"
        )

    def upload_solver(self):
        """Upload the solver binary to the instance."""
        binary = self.find_solver_binary()
        size_mb = binary.stat().st_size / (1024 * 1024)
        print(f"Uploading solver ({size_mb:.1f} MB)...")
        self._scp_to(binary, "/tmp/spinsat")
        self._ssh("chmod +x /tmp/spinsat")
        print("  Solver uploaded.")

    def upload_instances(self, instance_paths):
        """Upload CNF instances to the VM.

        If a GCS bucket is configured and instances exist there, the VM pulls
        from GCS directly (faster). Otherwise, tar+scp from local.
        """
        if self.bucket:
            return self._upload_via_gcs(instance_paths)
        return self._upload_via_scp(instance_paths)

    def _upload_via_scp(self, instance_paths):
        """Pack instances into a tarball and scp to VM."""
        print(f"Packing {len(instance_paths)} instances...")
        with tempfile.NamedTemporaryFile(suffix=".tar.gz", delete=False) as tf:
            tar_path = tf.name

        with tarfile.open(tar_path, "w:gz") as tar:
            for path in instance_paths:
                tar.add(path, arcname=os.path.basename(path))

        size_mb = os.path.getsize(tar_path) / (1024 * 1024)
        print(f"  Tarball: {size_mb:.1f} MB")
        print(f"Uploading instances...")
        self._scp_to(tar_path, "/tmp/instances.tar.gz")
        self._ssh("mkdir -p /tmp/instances && tar xzf /tmp/instances.tar.gz -C /tmp/instances")
        os.unlink(tar_path)
        print(f"  {len(instance_paths)} instances uploaded.")
        return "/tmp/instances"

    def _upload_via_gcs(self, instance_paths):
        """Sync instances to GCS bucket, then pull from VM."""
        bucket_uri = f"gs://{self.bucket}/instances"
        print(f"Syncing {len(instance_paths)} instances to {bucket_uri}...")

        # Upload any missing instances to GCS
        with tempfile.TemporaryDirectory() as tmpdir:
            for path in instance_paths:
                dst = os.path.join(tmpdir, os.path.basename(path))
                if not os.path.exists(dst):
                    shutil.copy2(path, dst)

            result = subprocess.run(
                ["gsutil", "-m", "rsync", tmpdir, bucket_uri],
                capture_output=True, text=True, timeout=300,
            )
            if result.returncode != 0:
                print(f"  GCS sync warning: {result.stderr}")
                print("  Falling back to SCP upload...")
                return self._upload_via_scp(instance_paths)

        # Pull from GCS on the VM
        print("  Pulling instances from GCS on VM...")
        self._ssh(
            f"mkdir -p /tmp/instances && gsutil -m cp '{bucket_uri}/*.cnf' /tmp/instances/",
            timeout=300,
        )
        print(f"  {len(instance_paths)} instances ready on VM.")
        return "/tmp/instances"

    def upload_worker_script(self):
        """Upload the cloud_worker.sh script to the instance."""
        self._scp_to(CLOUD_WORKER, "/tmp/cloud_worker.sh")
        self._ssh("chmod +x /tmp/cloud_worker.sh")

    @property
    def gcs_results_uri(self):
        """GCS URI for this run's results."""
        return f"gs://{self.results_bucket}/runs/{self._run_id}"

    # ------------------------------------------------------------------
    # Execution
    # ------------------------------------------------------------------

    def run(self, timeout, tag="", solver_args=None, total_instances=0):
        """Run the benchmark on the VM via nohup (survives SSH drops).

        The worker runs detached and writes results incrementally to
        /tmp/results.json after every instance completion. If SSH drops,
        results are still on the VM and can be recovered.
        """
        self._metrics = None
        print()
        print("=" * 60)
        print("CLOUD BENCHMARK EXECUTION")
        print("=" * 60)
        print(f"  Parallelism: {self.parallelism} (competition-faithful)")
        print(f"  Timeout: {timeout}s per instance")
        if solver_args:
            print(f"  Solver args: {' '.join(solver_args)}")
        print()

        # Launch worker detached via nohup so it survives SSH drops
        extra = " ".join(solver_args) if solver_args else ""
        gcs_uri = self.gcs_results_uri
        launch_cmd = (
            f"nohup sudo GCS_RESULTS_URI={gcs_uri} /tmp/cloud_worker.sh "
            f"/tmp/spinsat /tmp/instances {timeout} "
            f"{self.parallelism} /tmp/results.json {extra} "
            f"> /tmp/worker_stdout.log 2>&1 &"
        )
        self._ssh(launch_cmd, timeout=30)
        print("  Worker launched (detached via nohup)")

        # Start metrics reporting
        try:
            from monitoring import MetricsReporter
            self._metrics = MetricsReporter(
                instance_name=self.instance_name,
                zone=self.zone,
                project=self.project,
                labels={k: v for k, v in self._labels.items()},
            )
            self._metrics.start()
        except Exception as e:
            print(f"  Monitoring unavailable: {e}")
            self._metrics = None

        # Wait briefly for worker to initialize
        time.sleep(5)

        # Poll for progress, streaming output
        print("  Monitoring progress (Ctrl+C safe — worker continues on VM)...")
        last_line_count = 0
        consecutive_failures = 0
        max_consecutive_failures = 10
        poll_interval = 15
        ssh_timeout = 30

        while True:
            try:
                time.sleep(poll_interval)

                # Single SSH call to get status + log lines + process check
                poll_cmd = (
                    f"echo STATUS=$(cat /tmp/spinsat_status 2>/dev/null || echo unknown);"
                    f"echo RESULTS=$(sudo ls /tmp/spinsat_results/*.json 2>/dev/null | wc -l);"
                    f"echo PROC=$(pgrep -f cloud_worker.sh > /dev/null 2>&1 && echo running || echo stopped);"
                    f"tail -n +{last_line_count + 1} /tmp/worker_stdout.log 2>/dev/null | head -50"
                )
                result = self._ssh(poll_cmd, timeout=ssh_timeout)

                if result.returncode != 0:
                    consecutive_failures += 1
                    if consecutive_failures >= max_consecutive_failures:
                        print(f"\n  SSH failed {max_consecutive_failures}x consecutively. Giving up on polling.")
                        print(f"  Worker continues on VM: {self.instance_name}")
                        print(f"  Recover: --cloud-recover {self.instance_name} --cloud-zone {self.zone}")
                        break
                    print(f"  SSH poll returned non-zero (attempt {consecutive_failures}/{max_consecutive_failures})")
                    continue

                consecutive_failures = 0
                output = result.stdout

                # Parse status line
                status = "unknown"
                results_count = "?"
                proc_status = "unknown"
                log_lines = []

                for line in output.split("\n"):
                    if line.startswith("STATUS="):
                        status = line.split("=", 1)[1].strip()
                    elif line.startswith("RESULTS="):
                        results_count = line.split("=", 1)[1].strip()
                    elif line.startswith("PROC="):
                        proc_status = line.split("=", 1)[1].strip()
                    elif line.strip():
                        log_lines.append(line)

                # Update metrics with progress
                if self._metrics and results_count != "?" and total_instances > 0:
                    try:
                        self._metrics.set_progress(int(results_count), total_instances)
                    except (ValueError, TypeError):
                        pass

                # Print new log lines
                for line in log_lines:
                    print(line)
                last_line_count += len(log_lines)

                if status == "completed":
                    print(f"  Worker completed. ({results_count} results)")
                    if self._metrics:
                        self._metrics.stop()
                    break

                if proc_status == "stopped" and status != "completed":
                    print(f"  WARNING: Worker stopped but status='{status}' ({results_count} results)")
                    print("  Partial results available in /tmp/results.json")
                    break

            except subprocess.TimeoutExpired:
                consecutive_failures += 1
                if consecutive_failures >= max_consecutive_failures:
                    print(f"\n  SSH timed out {max_consecutive_failures}x consecutively. Giving up on polling.")
                    print(f"  Worker continues on VM: {self.instance_name}")
                    print(f"  Recover: --cloud-recover {self.instance_name} --cloud-zone {self.zone}")
                    break
                print(f"  SSH timeout (attempt {consecutive_failures}/{max_consecutive_failures}), retrying in {poll_interval}s...")

            except KeyboardInterrupt:
                raise

            except Exception as e:
                consecutive_failures += 1
                if consecutive_failures >= max_consecutive_failures:
                    print(f"\n  Polling failed {max_consecutive_failures}x. Giving up.")
                    print(f"  Worker continues on VM: {self.instance_name}")
                    print(f"  Recover: --cloud-recover {self.instance_name} --cloud-zone {self.zone}")
                    break
                print(f"  Poll error ({e}), retrying...")

        # Ensure metrics are stopped on any exit path
        if self._metrics:
            try:
                self._metrics.stop()
            except Exception:
                pass

        return "/tmp/results.json"

    def recover_results(self, run_id, tag, timeout_s):
        """Recover results from a running or completed VM.

        Use when SSH dropped during a run. The worker writes results
        incrementally, so partial results are always available.
        """
        print(f"Recovering results from {self.instance_name}...")

        # Check if results exist
        result = self._ssh(
            "ls -la /tmp/results.json 2>/dev/null && cat /tmp/spinsat_status 2>/dev/null || echo 'no results'",
            timeout=15,
        )
        print(f"  VM status: {result.stdout.strip() if hasattr(result, 'stdout') else 'unknown'}")

        return self.download_results("/tmp/results.json", run_id, tag, timeout_s)

    def download_results(self, remote_path, run_id, tag, timeout_s):
        """Download results from GCS (preferred) or VM via SCP (fallback)."""
        local_tmp = tempfile.NamedTemporaryFile(
            suffix=".json", delete=False, prefix="cloud_results_"
        )
        local_tmp.close()

        # Try GCS first — results survive VM deletion
        gcs_path = f"{self.gcs_results_uri}/results.json"
        try:
            result = subprocess.run(
                ["gsutil", "-q", "cp", gcs_path, local_tmp.name],
                capture_output=True, text=True, timeout=60,
            )
            if result.returncode == 0 and os.path.getsize(local_tmp.name) > 2:
                print(f"  Results downloaded from GCS: {gcs_path}")
            else:
                raise CloudBenchmarkError("GCS results empty or missing")
        except (CloudBenchmarkError, subprocess.TimeoutExpired, FileNotFoundError):
            print(f"  GCS download failed, falling back to SCP...")
            self._scp_from(remote_path, local_tmp.name)

        with open(local_tmp.name) as f:
            raw_instances = json.load(f)
        os.unlink(local_tmp.name)

        # Get CPU info from VM
        cpu_info = "unknown"
        try:
            result = self._ssh(
                "lscpu | grep 'Model name' | sed 's/.*: *//'",
                timeout=10,
            )
            if hasattr(result, "stdout") and result.stdout.strip():
                cpu_info = result.stdout.strip()
        except Exception:
            pass

        results = {
            "run_id": run_id,
            "tag": tag,
            "timestamp": datetime.now().isoformat(),
            "timeout_s": timeout_s,
            "solvers": ["spinsat"],
            "environment": {
                "type": "cloud",
                "provider": "gcp",
                "project": self.project,
                "machine_type": self.machine_type,
                "cpu_platform": f"Intel Ice Lake ({cpu_info})",
                "zone": self.zone,
                "spot": self.spot,
                "turbo_disabled": True,
                "parallelism": self.parallelism,
                "instance_name": self.instance_name,
            },
            "instances": raw_instances,
        }

        return results

    # ------------------------------------------------------------------
    # Dry run
    # ------------------------------------------------------------------

    def print_plan(self, instances, timeout):
        """Print what would happen without executing."""
        n = len(instances)
        est_runtime_s = (n / self.parallelism) * min(timeout, 300)
        est_runtime_h = est_runtime_s / 3600

        hourly_rate = 0.064 if self.spot else 0.29
        est_cost = est_runtime_h * hourly_rate

        print("=" * 60)
        print("DRY RUN — Cloud Benchmark Plan")
        print("=" * 60)
        print(f"  Instance:    {self.instance_name}")
        print(f"  Zone:        {self.zone}")
        print(f"  Machine:     {self.machine_type}")
        print(f"  CPU:         Intel Ice Lake (pinned)")
        print(f"  Spot:        {self.spot}")
        print(f"  Parallelism: {self.parallelism}")
        print(f"  Max life:    {self.max_hours}h")
        print()
        print(f"  Instances:   {n} CNF files")
        print(f"  Timeout:     {timeout}s per instance")
        print(f"  Est. runtime: {est_runtime_h:.1f}h (assuming avg 300s/instance)")
        print(f"  Est. cost:   ${est_cost:.2f} ({'spot' if self.spot else 'on-demand'})")
        print()

        # Check for existing instances
        try:
            result = self._gcloud([
                "compute", "instances", "list",
                "--filter", "name~spinsat-bench",
                "--format", "table(name,zone,status)",
            ])
            if result.stdout.strip():
                print("  ⚠ Existing benchmark instances found:")
                print(result.stdout)
        except Exception:
            pass

        # Check solver binary
        musl_path = PROJECT_ROOT / "target" / "x86_64-unknown-linux-musl" / "release" / "spinsat"
        if musl_path.exists():
            size_mb = musl_path.stat().st_size / (1024 * 1024)
            print(f"  Solver:      {musl_path.name} ({size_mb:.1f} MB)")
        else:
            print("  Solver:      NOT FOUND — will build musl target on launch")

        print()
        print("Run without --dry-run to execute.")

    # ------------------------------------------------------------------
    # Zombie cleanup
    # ------------------------------------------------------------------

    @staticmethod
    def cleanup_instances(project=GCP_PROJECT):
        """List and optionally delete leftover spinsat instances."""
        result = subprocess.run(
            [
                "gcloud", "compute", "instances", "list",
                "--filter", "name~spinsat",
                "--format", "table(name,zone,machineType.basename(),status,creationTimestamp)",
                "--project", project,
            ],
            capture_output=True, text=True, timeout=30,
        )
        if not result.stdout.strip():
            print("No spinsat instances found.")
            return

        print("SpinSAT instances:")
        print(result.stdout)

        # Also list orphaned disks not attached to any instance
        disk_result = subprocess.run(
            [
                "gcloud", "compute", "disks", "list",
                "--filter", "name~spinsat AND -users:*",
                "--format", "table(name,zone.basename(),sizeGb,type.basename(),status)",
                "--project", project,
            ],
            capture_output=True, text=True, timeout=30,
        )
        if disk_result.stdout.strip():
            print("\nOrphaned disks (not attached to any instance):")
            print(disk_result.stdout)
