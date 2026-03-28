# ---------------------------------------------------------------------------
# Notification Channels
# ---------------------------------------------------------------------------

resource "google_monitoring_notification_channel" "email" {
  display_name = "Sean Bearden (mobile)"
  type         = "email"
  project      = var.project

  labels = {
    email_address = var.notification_email
  }

  depends_on = [google_project_service.apis["monitoring.googleapis.com"]]
}

resource "google_monitoring_notification_channel" "pubsub" {
  display_name = "SpinSAT VM Alerts (Pub/Sub)"
  type         = "pubsub"
  project      = var.project

  labels = {
    topic = google_pubsub_topic.vm_alerts.id
  }

  depends_on = [
    google_project_service.apis["monitoring.googleapis.com"],
    google_pubsub_topic.vm_alerts,
  ]
}

# ---------------------------------------------------------------------------
# Alert: Benchmark Run Completed
# ---------------------------------------------------------------------------

resource "google_monitoring_alert_policy" "benchmark_completed" {
  display_name = "SpinSAT: Benchmark Run Completed"
  project      = var.project
  combiner     = "OR"
  enabled      = true

  documentation {
    content   = <<-EOT
      A SpinSAT benchmark run has completed successfully.

      Results are available in GCS: gs://spinsat-results/runs/

      Check locally: python3 scripts/benchmark_suite.py --list-suites
    EOT
    mime_type = "text/markdown"
  }

  conditions {
    display_name = "benchmark_completed > 0"

    condition_threshold {
      filter          = "resource.type = \"gce_instance\" AND metric.type = \"custom.googleapis.com/spinsat/benchmark_completed\""
      comparison      = "COMPARISON_GT"
      threshold_value = 0
      duration        = "0s"

      aggregations {
        alignment_period   = "60s"
        per_series_aligner = "ALIGN_MAX"
        group_by_fields    = ["metric.label.instance_name"]
      }

      trigger {
        count = 1
      }
    }
  }

  alert_strategy {
    auto_close = "1800s"
  }

  notification_channels = [
    google_monitoring_notification_channel.email.name,
  ]

  user_labels = {
    purpose = "benchmark-completion"
  }

  depends_on = [google_project_service.apis["monitoring.googleapis.com"]]
}

# ---------------------------------------------------------------------------
# Alert: Benchmark VM Idle (no heartbeat for 15 min)
# ---------------------------------------------------------------------------

resource "google_monitoring_alert_policy" "benchmark_idle" {
  display_name = "SpinSAT: Benchmark VM Idle (no heartbeat for 15min)"
  project      = var.project
  combiner     = "OR"
  enabled      = true

  documentation {
    content   = <<-EOT
      A SpinSAT benchmark VM has stopped sending heartbeat metrics for 15 minutes.
      This means the benchmark script has either completed, crashed, or the VM is
      idle and incurring charges.

      Action: Check the VM status and stop it if no longer needed.
      - List VMs: gcloud compute instances list --filter='name~spinsat-bench' --project=spinsat
      - Stop VM: gcloud compute instances stop <name> --zone=<zone> --project=spinsat
      - Recover results: python3 scripts/benchmark_suite.py --cloud-recover <name>
    EOT
    mime_type = "text/markdown"
  }

  conditions {
    display_name = "benchmark_active is absent for 15min"

    condition_absent {
      filter   = "resource.type = \"gce_instance\" AND metric.type = \"custom.googleapis.com/spinsat/benchmark_active\""
      duration = "900s"

      aggregations {
        alignment_period   = "300s"
        per_series_aligner = "ALIGN_MEAN"
        group_by_fields    = ["metric.label.instance_name"]
      }
    }
  }

  alert_strategy {
    auto_close = "1800s"
  }

  notification_channels = [
    google_monitoring_notification_channel.email.name,
    google_monitoring_notification_channel.pubsub.name,
  ]

  user_labels = {
    purpose = "benchmark-idle-detection"
  }

  depends_on = [google_project_service.apis["monitoring.googleapis.com"]]
}

# ---------------------------------------------------------------------------
# Dashboard: SpinSAT Benchmark Observability
# ---------------------------------------------------------------------------

resource "google_monitoring_dashboard" "benchmarks" {
  project        = var.project
  dashboard_json = file("${path.module}/dashboard-benchmarks.json")

  depends_on = [google_project_service.apis["monitoring.googleapis.com"]]
}
