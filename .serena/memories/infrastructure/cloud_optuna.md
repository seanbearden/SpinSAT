# Cloud Optuna — Distributed Tuning (2026-03-29)

## How It Works
`scripts/cloud_optuna.py` orchestrates distributed Optuna tuning on GCP:
1. Creates/verifies Cloud SQL study
2. Uploads solver binary + instances + scripts to GCS (`gs://spinsat-benchmarks/optuna/`)
3. Creates N spot VMs from pre-baked `spinsat-optuna` image family
4. Each VM startup script: activates venv, downloads from GCS, patches paths, runs optuna_tune.py
5. Workers share study via Cloud SQL PostgreSQL — each pulls independent trials
6. Local script polls progress (can detach — workers are autonomous)

## Launching
```bash
python3 scripts/optuna_tune.py \
    --campaign campaigns/<campaign>.yaml \
    --cloud --cloud-workers 4 --cloud-max-hours 12
```

## Monitoring
```bash
python3 scripts/optuna_tune.py --campaign campaigns/<campaign>.yaml --cloud --cloud-status

# Or query directly:
python3 -c "
import optuna
pw = open('optuna_studies/.db-password-spinsat-optuna').read().strip()
study = optuna.load_study('<study-name>', storage=f'postgresql://optuna:{pw}@34.57.20.164:5432/optuna')
print(f'{len([t for t in study.trials if t.state.name==\"COMPLETE\"])} complete')
if study.best_trial: print(f'Best PAR-2: {study.best_value:.2f}')
"
```

## VM Configuration
- Image: `spinsat-optuna` (pre-baked, project `spinsat`) — NOT stock Debian
- Machine: `c3-standard-4` (4 vCPUs, spot)
- Preemption: `--instance-termination-action STOP --restart-on-failure`
  - VMs restart automatically after preemption, re-run startup script, rejoin study
- Safety shutdown: `shutdown -h +{max_hours*60}` in startup script

## GCS Layout
```
gs://spinsat-benchmarks/optuna/
  spinsat              # musl binary
  campaign.yaml        # patched with local instance paths on VM
  instances/*.cnf      # solver instances
  scripts/             # optuna_tune.py, campaign_config.py, benchmark_suite.py
```

## Critical: Rebuild BOTH binaries before cloud tuning
```bash
cargo build --release                                    # local
cargo build --release --target x86_64-unknown-linux-musl  # cloud VMs use this
```

## Per-Instance Results
Completed (non-pruned) trials automatically record per-instance results to
Cloud SQL `spinsat_benchmarks.results` with run_id = `optuna_{study}_trial{N}`.
Pruned trials are NOT recorded (incomplete data).

## Current Campaign
- Study: `tune-general-sat2017`
- Instances: 120 (40 barthel + 40 komb + 40 qhid from sat2017)
- Goal: general parameters that work across all random families
- Config: `campaigns/tune_general_sat2017.yaml`

## Cleanup
```bash
python3 scripts/optuna_tune.py --campaign <campaign>.yaml --cloud --cloud-cleanup
```
