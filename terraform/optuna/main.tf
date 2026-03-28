terraform {
  required_version = ">= 1.5"
  required_providers {
    google = {
      source  = "hashicorp/google"
      version = "~> 5.0"
    }
  }
}

provider "google" {
  project = var.project
  region  = var.region
}

# ---------------------------------------------------------------------------
# Cloud SQL PostgreSQL — shared Optuna storage
# ---------------------------------------------------------------------------

resource "google_sql_database_instance" "optuna" {
  name             = "spinsat-optuna"
  database_version = "POSTGRES_15"
  region           = var.region

  settings {
    tier              = var.db_tier
    availability_type = "ZONAL"
    disk_size         = 10
    disk_type         = "PD_SSD"

    ip_configuration {
      ipv4_enabled = true
      authorized_networks {
        name  = "all"
        value = "0.0.0.0/0"
      }
    }

    database_flags {
      name  = "max_connections"
      value = "100"
    }

    backup_configuration {
      enabled = false
    }
  }

  deletion_protection = false
}

resource "google_sql_database" "optuna" {
  name     = "optuna"
  instance = google_sql_database_instance.optuna.name
}

resource "google_sql_user" "optuna" {
  name     = "optuna"
  instance = google_sql_database_instance.optuna.name
  password = var.db_password
}

# ---------------------------------------------------------------------------
# GCS bucket
# ---------------------------------------------------------------------------

resource "google_storage_bucket" "benchmarks" {
  name     = var.gcs_bucket
  location = var.region

  uniform_bucket_level_access = true
  force_destroy               = false
}

# ---------------------------------------------------------------------------
# Pre-baked worker image (built by scripts/build_worker_image.sh)
# ---------------------------------------------------------------------------

data "google_compute_image" "optuna_worker" {
  family  = "spinsat-optuna"
  project = var.project
}

# ---------------------------------------------------------------------------
# Instance template — spot VMs with pre-baked image
# ---------------------------------------------------------------------------

resource "google_compute_instance_template" "optuna_worker" {
  name_prefix  = "spinsat-optuna-worker-"
  machine_type = var.machine_type

  scheduling {
    preemptible                 = true
    provisioning_model          = "SPOT"
    instance_termination_action = "STOP"
    automatic_restart           = false
    max_run_duration {
      seconds = var.max_run_hours * 3600
    }
  }

  disk {
    source_image = data.google_compute_image.optuna_worker.self_link
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
    startup-script = templatefile("${path.module}/worker_startup.sh.tpl", {
      db_url        = "postgresql://optuna:${var.db_password}@${google_sql_database_instance.optuna.public_ip_address}:5432/optuna"
      gcs_bucket    = var.gcs_bucket
      study_name    = var.study_name
      campaign_yaml = var.campaign_yaml
      n_trials      = var.n_trials
      max_hours     = var.max_run_hours
    })
  }

  lifecycle {
    create_before_destroy = true
  }
}

# ---------------------------------------------------------------------------
# Managed Instance Group — auto-replaces preempted workers
# ---------------------------------------------------------------------------

resource "google_compute_region_instance_group_manager" "optuna_workers" {
  name               = "spinsat-optuna-workers"
  base_instance_name = "spinsat-optuna-w"
  region             = var.region

  distribution_policy_zones         = var.zones
  distribution_policy_target_shape  = "EVEN"

  version {
    instance_template = google_compute_instance_template.optuna_worker.self_link
  }

  target_size = var.worker_count

  update_policy {
    type                  = "PROACTIVE"
    minimal_action        = "REPLACE"
    max_surge_fixed       = length(var.zones)
    max_unavailable_fixed = length(var.zones)
  }
}

# ---------------------------------------------------------------------------
# Outputs
# ---------------------------------------------------------------------------

output "db_ip" {
  description = "Cloud SQL public IP"
  value       = google_sql_database_instance.optuna.public_ip_address
}

output "db_connection_url" {
  description = "PostgreSQL connection URL (for local monitoring)"
  value       = "postgresql://optuna:***@${google_sql_database_instance.optuna.public_ip_address}:5432/optuna"
}

output "mig_name" {
  description = "Managed Instance Group name"
  value       = google_compute_region_instance_group_manager.optuna_workers.name
}

output "worker_count" {
  description = "Current target worker count"
  value       = var.worker_count
}

output "scale_command" {
  description = "Command to scale workers up/down"
  value       = "gcloud compute instance-groups managed resize ${google_compute_region_instance_group_manager.optuna_workers.name} --size=<N> --region=${var.region} --project=${var.project}"
}

output "stop_command" {
  description = "Command to stop all workers (set size to 0)"
  value       = "gcloud compute instance-groups managed resize ${google_compute_region_instance_group_manager.optuna_workers.name} --size=0 --region=${var.region} --project=${var.project}"
}
