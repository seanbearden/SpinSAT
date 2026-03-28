"""Cloud Function: Auto-stop idle SpinSAT benchmark VMs.

Triggered by Cloud Monitoring alert via Pub/Sub. When a benchmark VM
stops sending heartbeat metrics for 15 minutes, this function stops
the VM to prevent cost accumulation.

Only acts on VMs with the label purpose=benchmark and name matching
spinsat-bench-*. Results are safe in GCS before the VM is stopped.
"""

import base64
import json
import logging

import functions_framework
from google.cloud import compute_v1

logger = logging.getLogger(__name__)
logging.basicConfig(level=logging.INFO)


@functions_framework.cloud_event
def auto_stop_vm(cloud_event):
    """Handle Cloud Monitoring alert and stop idle benchmark VMs."""
    # Decode the Pub/Sub message
    data = cloud_event.data
    message_data = base64.b64decode(data["message"]["data"]).decode("utf-8")
    alert = json.loads(message_data)

    logger.info(f"Received alert: {alert.get('incident', {}).get('policy_name', 'unknown')}")

    incident = alert.get("incident", {})
    state = incident.get("state", "")

    # Only act on opened incidents (not resolved)
    if state != "open":
        logger.info(f"Ignoring alert with state: {state}")
        return

    # Extract instance info from the alert condition
    resource_labels = incident.get("resource", {}).get("labels", {})
    metric_labels = incident.get("metric", {}).get("labels", {})

    instance_name = metric_labels.get("instance_name", "")
    project = resource_labels.get("project_id", "spinsat")
    zone = resource_labels.get("zone", "")

    if not instance_name or not zone:
        logger.warning(f"Missing instance_name or zone in alert: {incident}")
        return

    # Safety check: only stop spinsat-bench-* VMs
    if not instance_name.startswith("spinsat-bench-"):
        logger.warning(f"Refusing to stop non-benchmark VM: {instance_name}")
        return

    logger.info(f"Auto-stopping idle VM: {instance_name} (zone: {zone}, project: {project})")

    try:
        client = compute_v1.InstancesClient()
        instance = client.get(project=project, zone=zone, instance=instance_name)

        # Verify the purpose=benchmark label
        labels = instance.labels or {}
        if labels.get("purpose") != "benchmark":
            logger.warning(
                f"VM {instance_name} missing purpose=benchmark label. "
                f"Labels: {labels}. Skipping."
            )
            return

        # Only stop if RUNNING (not already STOPPED/TERMINATED)
        if instance.status != "RUNNING":
            logger.info(f"VM {instance_name} is already {instance.status}. No action needed.")
            return

        # Stop the VM
        operation = client.stop(project=project, zone=zone, instance=instance_name)
        logger.info(
            f"Stop operation initiated for {instance_name}: "
            f"operation={operation.name}"
        )

    except Exception as e:
        logger.error(f"Failed to stop VM {instance_name}: {e}")
        raise
