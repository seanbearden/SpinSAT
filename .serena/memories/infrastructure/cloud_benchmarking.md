# Cloud Benchmarking (2026-03-29)

## Running Benchmarks on GCP
```bash
python3 scripts/benchmark_suite.py \
    --instances ~/gt/spinsat/benchmarks/sat2017/*barthel*.cnf \
    --timeout 300 --record --force \
    --tag v0.5.3-barthel-tuned \
    --solver-args "--beta 79.59 --gamma 0.188 -m rk4" \
    --cloud --cloud-spot
```

## NEVER run experiments locally
All benchmark/solver experiments go to GCP cloud VMs. Local machine is ONLY for
quick smoke tests (1-2 instances, <10s). This was an explicit correction from Sean.

## VM Details
- Machine: n2-highcpu-8 (Intel Ice Lake, pinned CPU platform)
- Spot pricing: ~$0.034/hr
- Parallelism: 8 concurrent solver invocations per VM
- Auto-cleanup: VM deleted after run completes

## Results Recording
Currently records to local SQLite benchmarks.db (need to update to Cloud SQL).
The `record_to_db` function needs missing columns added to match current schema
(restart_strategy, preprocessing, cli_command, peak_xl_max, final_dt, etc.).

## Instance Hash
`compute_instance_hash()` extracts GBD hash from filename prefix pattern
`<32-char-hex>-<name>.cnf`. Falls back to SHA-256 for non-GBD files.
This ensures results match competition_results table for head-to-head queries.

## Head-to-Head Query (works from Cloud SQL)
```sql
SELECT ru.tag, COUNT(*) as n,
    SUM(CASE WHEN r.status='SATISFIABLE' THEN 1 ELSE 0 END) as solved,
    ROUND(AVG(CASE WHEN r.status='SATISFIABLE' THEN r.time_s
              ELSE 2*ru.timeout_s END)::numeric, 1) as par2,
    ROUND(AVG(comp.best_time)::numeric, 3) as comp_par2
FROM results r
JOIN runs ru USING(run_id)
JOIN instance_files if2 ON r.instance_hash = if2.hash
JOIN (SELECT instance_hash, MIN(time_s) as best_time
      FROM competition_results WHERE status='SATISFIABLE'
      GROUP BY instance_hash) comp USING(instance_hash)
WHERE if2.value LIKE '%barthel%'
GROUP BY ru.tag ORDER BY par2;
```

## Budget
$150/month total. Cloud SQL ~$25/month. Remaining $125 for spot VMs.
