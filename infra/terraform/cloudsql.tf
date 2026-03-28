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
      enabled = false
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

  deletion_protection = false

  depends_on = [google_project_service.apis["sqladmin.googleapis.com"]]
}

resource "google_sql_database" "optuna" {
  name     = "optuna"
  instance = google_sql_database_instance.optuna.name
  project  = var.project
}

resource "google_sql_user" "optuna" {
  name     = "optuna"
  instance = google_sql_database_instance.optuna.name
  project  = var.project
  password = random_password.optuna_db.result
}

resource "random_password" "optuna_db" {
  length  = 32
  special = false
}
