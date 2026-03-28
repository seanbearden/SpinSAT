variable "project" {
  description = "GCP project ID"
  type        = string
  default     = "spinsat"
}

variable "region" {
  description = "GCP region"
  type        = string
  default     = "us-central1"
}

variable "zones" {
  description = "Zones to spread workers across (preemption resilience)"
  type        = list(string)
  default     = ["us-central1-a", "us-central1-b", "us-central1-c"]
}

variable "worker_count" {
  description = "Number of spot VM workers"
  type        = number
  default     = 2
}

variable "machine_type" {
  description = "Worker VM machine type"
  type        = string
  default     = "c3-standard-4"
}

variable "max_run_hours" {
  description = "Max worker lifetime in hours"
  type        = number
  default     = 12
}

variable "db_tier" {
  description = "Cloud SQL instance tier"
  type        = string
  default     = "db-g1-small"
}

variable "db_password" {
  description = "Database password for optuna user"
  type        = string
  sensitive   = true
}

variable "gcs_bucket" {
  description = "GCS bucket for solver + instances"
  type        = string
  default     = "spinsat-benchmarks"
}

variable "study_name" {
  description = "Optuna study name"
  type        = string
}

variable "campaign_yaml" {
  description = "Campaign YAML filename in GCS"
  type        = string
  default     = "campaign.yaml"
}

variable "n_trials" {
  description = "Total number of Optuna trials"
  type        = number
  default     = 200
}
