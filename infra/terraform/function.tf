# ---------------------------------------------------------------------------
# Cloud Function — auto-stop idle benchmark VMs
# ---------------------------------------------------------------------------

data "archive_file" "auto_stop_vm" {
  type        = "zip"
  source_dir  = "${path.module}/../auto-stop-vm"
  output_path = "${path.module}/.build/auto-stop-vm.zip"
}

resource "google_storage_bucket_object" "auto_stop_vm_source" {
  name   = "cloud-functions/auto-stop-vm-${data.archive_file.auto_stop_vm.output_md5}.zip"
  bucket = google_storage_bucket.benchmarks.name
  source = data.archive_file.auto_stop_vm.output_path
}

resource "google_cloudfunctions2_function" "auto_stop_vm" {
  name     = "spinsat-auto-stop-vm"
  location = var.region
  project  = var.project

  build_config {
    runtime     = "python312"
    entry_point = "auto_stop_vm"

    source {
      storage_source {
        bucket = google_storage_bucket.benchmarks.name
        object = google_storage_bucket_object.auto_stop_vm_source.name
      }
    }
  }

  service_config {
    max_instance_count = 1
    timeout_seconds    = 60
    available_memory   = "256M"
  }

  event_trigger {
    trigger_region = var.region
    event_type     = "google.cloud.pubsub.topic.v1.messagePublished"
    pubsub_topic   = google_pubsub_topic.vm_alerts.id
  }

  depends_on = [
    google_project_service.apis["cloudfunctions.googleapis.com"],
    google_project_service.apis["cloudbuild.googleapis.com"],
  ]
}
