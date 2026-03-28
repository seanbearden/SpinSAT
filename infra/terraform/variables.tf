variable "project" {
  description = "GCP project ID"
  type        = string
  default     = "spinsat"
}

variable "region" {
  description = "Default GCP region"
  type        = string
  default     = "us-central1"
}

variable "zone" {
  description = "Default GCP zone"
  type        = string
  default     = "us-central1-a"
}

# -- Notification channels --------------------------------------------------

variable "notification_email" {
  description = "Email address for monitoring alert notifications"
  type        = string
}

# -- Cloud SQL ---------------------------------------------------------------

variable "cloud_sql_tier" {
  description = "Cloud SQL machine tier"
  type        = string
  default     = "db-g1-small"
}

variable "cloud_sql_password" {
  description = "Optuna DB user password. If empty, a random password is generated (new project). For existing projects, pass the password from optuna_studies/.db-password-spinsat-optuna."
  type        = string
  default     = ""
  sensitive   = true
}

variable "cloud_sql_authorized_networks" {
  description = "CIDR ranges allowed to reach Cloud SQL (benchmark VMs connect directly)"
  type        = list(object({ name = string, value = string }))
  default = [
    { name = "all", value = "0.0.0.0/0" }
  ]
}
