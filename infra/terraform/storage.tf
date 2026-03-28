# ---------------------------------------------------------------------------
# GCS Buckets
# ---------------------------------------------------------------------------

resource "google_storage_bucket" "benchmarks" {
  name     = "spinsat-benchmarks"
  location = var.region
  project  = var.project

  uniform_bucket_level_access = true
  public_access_prevention    = "enforced"

  lifecycle_rule {
    condition { age = 90 }
    action { type = "Delete" }
  }

  depends_on = [google_project_service.apis["storage.googleapis.com"]]
}

resource "google_storage_bucket" "results" {
  name     = "spinsat-results"
  location = var.region
  project  = var.project

  uniform_bucket_level_access = true
  public_access_prevention    = "enforced"

  versioning { enabled = true }

  depends_on = [google_project_service.apis["storage.googleapis.com"]]
}
