# ---------------------------------------------------------------------------
# Cloud SQL — Optuna shared storage
# ---------------------------------------------------------------------------

resource "google_sql_database_instance" "optuna" {
  name             = "spinsat-optuna"
  database_version = "POSTGRES_15"
  region           = var.region
  project          = var.project

  settings {
    tier              = var.cloud_sql_tier
    availability_type = "ZONAL"
    disk_size         = 10
    disk_type         = "PD_SSD"

    backup_configuration {
      enabled                        = true
      start_time                     = "04:00"
      point_in_time_recovery_enabled = false
      transaction_log_retention_days = 1
      backup_retention_settings {
        retained_backups = 7
      }
    }

    ip_configuration {
      ipv4_enabled = true

      dynamic "authorized_networks" {
        for_each = var.cloud_sql_authorized_networks
        content {
          name  = authorized_networks.value.name
          value = authorized_networks.value.value
        }
      }
    }

    database_flags {
      name  = "max_connections"
      value = "100"
    }
  }

  deletion_protection = true

  depends_on = [google_project_service.apis["sqladmin.googleapis.com"]]
}

resource "google_sql_database" "optuna" {
  name     = "optuna"
  instance = google_sql_database_instance.optuna.name
  project  = var.project
}

# Use provided password for existing projects, generate for new ones.
resource "random_password" "optuna_db" {
  length  = 32
  special = false
}

locals {
  optuna_db_password = var.cloud_sql_password != "" ? var.cloud_sql_password : random_password.optuna_db.result
}

resource "google_sql_user" "optuna" {
  name     = "optuna"
  instance = google_sql_database_instance.optuna.name
  project  = var.project
  password = local.optuna_db_password
}

# ---------------------------------------------------------------------------
# Benchmarks database — single source of truth for all benchmark results
# ---------------------------------------------------------------------------

resource "google_sql_database" "benchmarks" {
  name     = "spinsat_benchmarks"
  instance = google_sql_database_instance.optuna.name
  project  = var.project
}

resource "google_sql_user" "benchmarks" {
  name     = "benchmarks"
  instance = google_sql_database_instance.optuna.name
  project  = var.project
  password = local.optuna_db_password  # reuse same password for simplicity
}
