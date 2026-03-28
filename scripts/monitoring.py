#!/usr/bin/env python3
"""
GCP Cloud Monitoring custom metrics module for SpinSAT.

Reusable module that any script can import to push custom metrics to
Cloud Monitoring. Designed for benchmark VM monitoring but extensible
to other use cases.

Usage:
    from monitoring import MetricsReporter

    reporter = MetricsReporter(
        instance_name="spinsat-bench-20260328-120000",
        zone="us-central1-a",
    )
    reporter.start()  # Starts background heartbeat thread
    reporter.set_progress(5, 20)  # 5 of 20 instances done
    reporter.stop()   # Sets benchmark_active=0, stops heartbeat

The module gracefully degrades if Cloud Monitoring is unavailable
(missing credentials, network issues, etc.) — it logs warnings
but never raises exceptions that would interrupt the benchmark.
"""

import atexit
import logging
import threading
import time

logger = logging.getLogger(__name__)

# Metric type prefix
METRIC_PREFIX = "custom.googleapis.com/spinsat"

# Metric descriptors
METRICS = {
    "benchmark_active": {
        "type": f"{METRIC_PREFIX}/benchmark_active",
        "description": "Whether a benchmark is actively running (1=running, 0=idle)",
        "metric_kind": "GAUGE",
        "value_type": "INT64",
    },
    "benchmark_progress": {
        "type": f"{METRIC_PREFIX}/benchmark_progress",
        "description": "Fraction of instances completed (0.0 to 1.0)",
        "metric_kind": "GAUGE",
        "value_type": "DOUBLE",
    },
}


def _get_monitoring_client():
    """Lazily import and create the Cloud Monitoring client."""
    try:
        from google.cloud import monitoring_v3
        return monitoring_v3.MetricServiceClient()
    except ImportError:
        logger.warning(
            "google-cloud-monitoring not installed. "
            "Install with: pip install google-cloud-monitoring"
        )
        return None
    except Exception as e:
        logger.warning(f"Could not create monitoring client: {e}")
        return None


class MetricsReporter:
    """Pushes custom metrics to GCP Cloud Monitoring on a heartbeat interval.

    Thread-safe. All public methods can be called from any thread.
    Failures are logged but never raised — monitoring is best-effort.
    """

    def __init__(
        self,
        instance_name,
        zone="us-central1-a",
        project="spinsat",
        heartbeat_interval=60,
        labels=None,
    ):
        self.instance_name = instance_name
        self.zone = zone
        self.project = project
        self.heartbeat_interval = heartbeat_interval
        self.extra_labels = labels or {}

        self._client = None
        self._project_name = f"projects/{project}"
        self._lock = threading.Lock()
        self._active = False
        self._progress = 0.0
        self._completed = 0
        self._total = 0
        self._heartbeat_thread = None
        self._stop_event = threading.Event()

    def start(self):
        """Start the heartbeat thread and set benchmark_active=1."""
        self._client = _get_monitoring_client()
        if self._client is None:
            logger.warning("Monitoring disabled — no client available")
            return

        self._ensure_metric_descriptors()

        with self._lock:
            self._active = True

        self._push_metrics()

        self._stop_event.clear()
        self._heartbeat_thread = threading.Thread(
            target=self._heartbeat_loop,
            daemon=True,
            name="metrics-heartbeat",
        )
        self._heartbeat_thread.start()
        atexit.register(self.stop)
        logger.info(
            f"Monitoring started: {self.instance_name} "
            f"(heartbeat every {self.heartbeat_interval}s)"
        )

    def stop(self):
        """Set benchmark_active=0 and stop the heartbeat thread."""
        with self._lock:
            self._active = False

        self._push_metrics()
        self._stop_event.set()

        if self._heartbeat_thread and self._heartbeat_thread.is_alive():
            self._heartbeat_thread.join(timeout=5)

        logger.info(f"Monitoring stopped: {self.instance_name}")

    def set_progress(self, completed, total):
        """Update the progress metric. Thread-safe."""
        with self._lock:
            self._completed = completed
            self._total = total
            self._progress = completed / total if total > 0 else 0.0

    def _heartbeat_loop(self):
        """Background thread that pushes metrics on a fixed interval."""
        while not self._stop_event.wait(self.heartbeat_interval):
            self._push_metrics()

    def _push_metrics(self):
        """Push all metrics to Cloud Monitoring. Never raises."""
        if self._client is None:
            return

        try:
            from google.cloud import monitoring_v3
            from google.protobuf import timestamp_pb2
            import datetime

            now = datetime.datetime.now(datetime.timezone.utc)
            timestamp = timestamp_pb2.Timestamp()
            timestamp.FromDatetime(now)

            with self._lock:
                active_value = 1 if self._active else 0
                progress_value = self._progress

            resource = monitoring_v3.MonitoredResource(
                type="gce_instance",
                labels={
                    "project_id": self.project,
                    "instance_id": self.instance_name,
                    "zone": self.zone,
                },
            )

            metric_labels = {
                "instance_name": self.instance_name,
                **self.extra_labels,
            }

            # benchmark_active
            active_series = monitoring_v3.TimeSeries(
                metric=monitoring_v3.Metric(
                    type=METRICS["benchmark_active"]["type"],
                    labels=metric_labels,
                ),
                resource=resource,
                points=[
                    monitoring_v3.Point(
                        interval=monitoring_v3.TimeInterval(end_time=timestamp),
                        value=monitoring_v3.TypedValue(int64_value=active_value),
                    )
                ],
            )

            # benchmark_progress
            progress_series = monitoring_v3.TimeSeries(
                metric=monitoring_v3.Metric(
                    type=METRICS["benchmark_progress"]["type"],
                    labels=metric_labels,
                ),
                resource=resource,
                points=[
                    monitoring_v3.Point(
                        interval=monitoring_v3.TimeInterval(end_time=timestamp),
                        value=monitoring_v3.TypedValue(double_value=progress_value),
                    )
                ],
            )

            self._client.create_time_series(
                request={
                    "name": self._project_name,
                    "time_series": [active_series, progress_series],
                }
            )

        except Exception as e:
            logger.warning(f"Failed to push metrics: {e}")

    def _ensure_metric_descriptors(self):
        """Create metric descriptors if they don't exist."""
        if self._client is None:
            return

        try:
            from google.cloud import monitoring_v3
            from google.api import metric_pb2, label_pb2

            for name, spec in METRICS.items():
                descriptor = metric_pb2.MetricDescriptor(
                    type=spec["type"],
                    metric_kind=getattr(
                        metric_pb2.MetricDescriptor.MetricKind,
                        spec["metric_kind"],
                    ),
                    value_type=getattr(
                        metric_pb2.MetricDescriptor.ValueType,
                        spec["value_type"],
                    ),
                    description=spec["description"],
                    labels=[
                        label_pb2.LabelDescriptor(
                            key="instance_name",
                            value_type=label_pb2.LabelDescriptor.ValueType.STRING,
                            description="VM instance name",
                        ),
                    ],
                )
                try:
                    self._client.create_metric_descriptor(
                        request={
                            "name": self._project_name,
                            "metric_descriptor": descriptor,
                        }
                    )
                    logger.info(f"Created metric descriptor: {spec['type']}")
                except Exception:
                    # Already exists or permission issue — either way, continue
                    pass

        except Exception as e:
            logger.warning(f"Could not ensure metric descriptors: {e}")
