# ---------------------------------------------------------------------------
# Pub/Sub — VM alert routing
# ---------------------------------------------------------------------------

resource "google_pubsub_topic" "vm_alerts" {
  name    = "spinsat-vm-alerts"
  project = var.project

  depends_on = [google_project_service.apis["pubsub.googleapis.com"]]
}
