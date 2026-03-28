# ---------------------------------------------------------------------------
# Import blocks for adopting existing resources.
#
# These are no-ops on a fresh project (Terraform skips imports when the
# resource doesn't exist in the target). On the current spinsat project
# they let Terraform adopt what was created manually / by scripts.
#
# After the first successful `terraform apply`, these blocks can stay
# harmlessly or be removed — they only fire once per resource.
# ---------------------------------------------------------------------------

import {
  to = google_storage_bucket.benchmarks
  id = "spinsat/spinsat-benchmarks"
}

import {
  to = google_storage_bucket.results
  id = "spinsat/spinsat-results"
}

import {
  to = google_pubsub_topic.vm_alerts
  id = "projects/spinsat/topics/spinsat-vm-alerts"
}

import {
  to = google_sql_database_instance.optuna
  id = "projects/spinsat/instances/spinsat-optuna"
}

import {
  to = google_sql_database.optuna
  id = "projects/spinsat/instances/spinsat-optuna/databases/optuna"
}

import {
  to = google_sql_user.optuna
  id = "spinsat/spinsat-optuna/optuna"
}

import {
  to = google_cloudfunctions2_function.auto_stop_vm
  id = "projects/spinsat/locations/us-central1/functions/spinsat-auto-stop-vm"
}

import {
  to = google_monitoring_notification_channel.email
  id = "projects/spinsat/notificationChannels/954340860329199977"
}

import {
  to = google_monitoring_notification_channel.pubsub
  id = "projects/spinsat/notificationChannels/16562788783794802913"
}

import {
  to = google_monitoring_alert_policy.benchmark_completed
  id = "projects/spinsat/alertPolicies/8622688158162231134"
}

import {
  to = google_monitoring_alert_policy.benchmark_idle
  id = "projects/spinsat/alertPolicies/3354024007832841517"
}
