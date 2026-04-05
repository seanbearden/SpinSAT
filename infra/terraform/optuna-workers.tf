# ---------------------------------------------------------------------------
# Optuna distributed tuning workers
# Pre-baked image + MIG for preemption-resilient spot VMs
# ---------------------------------------------------------------------------

variable "optuna_worker_count" {
  description = "Number of Optuna spot VM workers (set to 0 to stop all)"
  type        = number
  default     = 0
}

variable "optuna_machine_type" {
  description = "Worker VM machine type"
  type        = string
  default     = "c3-standard-4"
}

variable "optuna_max_run_hours" {
  description = "Max worker lifetime in hours"
  type        = number
  default     = 12
}

variable "optuna_zones" {
  description = "Zones to spread workers across (preemption resilience)"
  type        = list(string)
  default     = ["us-central1-a", "us-central1-b", "us-central1-c"]
}

variable "optuna_study_name" {
  description = "Optuna study name"
  type        = string
  default     = ""
}

variable "optuna_campaign_yaml" {
  description = "Campaign YAML filename in GCS optuna/ prefix"
  type        = string
  default     = "campaign.yaml"
}

variable "optuna_n_trials" {
  description = "Total number of Optuna trials"
  type        = number
  default     = 200
}

# ---------------------------------------------------------------------------
# Pre-baked worker image (built by scripts/build_worker_image.sh)
# ---------------------------------------------------------------------------

data "google_compute_image" "optuna_worker" {
  count   = var.optuna_worker_count > 0 ? 1 : 0
  family  = "spinsat-optuna"
  project = var.project
}

# ---------------------------------------------------------------------------
# Instance template — spot VMs with pre-baked image, fast startup
# ---------------------------------------------------------------------------

resource "google_compute_instance_template" "optuna_worker" {
  count        = var.optuna_worker_count > 0 ? 1 : 0
  name_prefix  = "spinsat-optuna-worker-"
  machine_type = var.optuna_machine_type
  project      = var.project

  scheduling {
    preemptible                 = true
    provisioning_model          = "SPOT"
    instance_termination_action = "STOP"
    automatic_restart           = false
    # max_run_duration not supported with MIG — startup script has shutdown -h safety net
  }

  disk {
    source_image = data.google_compute_image.optuna_worker[0].self_link
    disk_size_gb = 30
    disk_type    = "pd-ssd"
    auto_delete  = true
    boot         = true
  }

  network_interface {
    network = "default"
    access_config {}
  }

  service_account {
    scopes = ["storage-ro", "sql-admin", "logging-write"]
  }

  metadata = {
    startup-script = templatefile("${path.module}/optuna-worker-startup.sh.tpl", {
      db_url        = "postgresql://optuna:${local.optuna_db_password}@${google_sql_database_instance.optuna.public_ip_address}:5432/optuna"
      gcs_bucket    = google_storage_bucket.benchmarks.name
      study_name    = var.optuna_study_name
      campaign_yaml = var.optuna_campaign_yaml
      n_trials      = var.optuna_n_trials
      max_hours     = var.optuna_max_run_hours
    })
  }

  lifecycle {
    create_before_destroy = true
  }

  depends_on = [
    google_sql_database_instance.optuna,
    google_sql_user.optuna,
    google_sql_database.optuna,
  ]
}

# ---------------------------------------------------------------------------
# Managed Instance Group — auto-replaces preempted workers
# ---------------------------------------------------------------------------

resource "google_compute_region_instance_group_manager" "optuna_workers" {
  count              = var.optuna_worker_count > 0 ? 1 : 0
  name               = "spinsat-optuna-workers"
  base_instance_name = "spinsat-optuna-w"
  region             = var.region
  project            = var.project

  distribution_policy_zones        = var.optuna_zones
  distribution_policy_target_shape = "EVEN"

  version {
    instance_template = google_compute_instance_template.optuna_worker[0].self_link
  }

  target_size = var.optuna_worker_count

  update_policy {
    type                  = "PROACTIVE"
    minimal_action        = "REPLACE"
    max_surge_fixed       = length(var.optuna_zones)
    max_unavailable_fixed = length(var.optuna_zones)
  }
}
