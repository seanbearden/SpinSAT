output "benchmarks_bucket" {
  value = google_storage_bucket.benchmarks.url
}

output "results_bucket" {
  value = google_storage_bucket.results.url
}

output "cloud_sql_ip" {
  value = google_sql_database_instance.optuna.public_ip_address
}

output "cloud_sql_connection_name" {
  value = google_sql_database_instance.optuna.connection_name
}

output "optuna_db_password" {
  value     = local.optuna_db_password
  sensitive = true
}

output "auto_stop_function_url" {
  value = google_cloudfunctions2_function.auto_stop_vm.url
}

output "pubsub_topic" {
  value = google_pubsub_topic.vm_alerts.id
}
