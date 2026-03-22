#!/bin/bash
# cloud_worker.sh — Runs on the GCP VM to execute benchmarks under
# competition-faithful conditions (8-way parallel, core-pinned, turbo off).
#
# Usage: cloud_worker.sh <solver> <instances_dir> <timeout> <parallelism> <results_file>

set -euo pipefail

SOLVER="$1"
INSTANCES_DIR="$2"
TIMEOUT="$3"
PARALLELISM="${4:-8}"
RESULTS_FILE="$5"

# --- Competition-faithful CPU setup ---
setup_cpu() {
    local turbo_disabled=false

    # Method 1: Intel pstate driver
    local pstate_path="/sys/devices/system/cpu/intel_pstate/no_turbo"
    if [ -f "$pstate_path" ]; then
        echo 1 > "$pstate_path" 2>/dev/null && turbo_disabled=true
    fi

    # Method 2: ACPI cpufreq / generic MSR (GCP VMs often use this)
    local boost_path="/sys/devices/system/cpu/cpufreq/boost"
    if [ -f "$boost_path" ] && [ "$turbo_disabled" = false ]; then
        echo 0 > "$boost_path" 2>/dev/null && turbo_disabled=true
    fi

    # Method 3: MSR-based disable (works on most Intel CPUs)
    if [ "$turbo_disabled" = false ] && command -v wrmsr >/dev/null 2>&1; then
        # Bit 38 of MSR 0x1a0 disables turbo
        wrmsr -a 0x1a0 0x4000850089 2>/dev/null && turbo_disabled=true
    elif [ "$turbo_disabled" = false ] && [ -f /dev/cpu/0/msr ]; then
        # Try installing msr-tools
        yum install -y msr-tools 2>/dev/null || apt-get install -y msr-tools 2>/dev/null || true
        modprobe msr 2>/dev/null || true
        if command -v wrmsr >/dev/null 2>&1; then
            wrmsr -a 0x1a0 0x4000850089 2>/dev/null && turbo_disabled=true
        fi
    fi

    if [ "$turbo_disabled" = true ]; then
        echo "Turbo boost: DISABLED"
    else
        echo "Turbo boost: WARNING — could not disable (times may be optimistic)"
    fi

    # Set performance governor on all cores
    local set_count=0
    for gov in /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor; do
        [ -f "$gov" ] && echo performance > "$gov" 2>/dev/null && set_count=$((set_count + 1))
    done
    echo "CPU governor: set 'performance' on $set_count cores"
}

setup_cpu

# --- Build job queue ---
find "$INSTANCES_DIR" -name "*.cnf" -type f | sort > /tmp/job_queue.txt
TOTAL=$(wc -l < /tmp/job_queue.txt)
echo "Job queue: $TOTAL instances, $PARALLELISM parallel workers, timeout=${TIMEOUT}s"

RESULTS_TMP=$(mktemp -d)
PROGRESS_FILE=$(mktemp)
echo "0" > "$PROGRESS_FILE"

# --- Worker function: runs solver on assigned instances ---
run_worker() {
    local core_id=$1
    local worker_queue=$2

    while IFS= read -r cnf_path || [ -n "$cnf_path" ]; do
        [ -z "$cnf_path" ] && continue
        local instance_name
        instance_name=$(basename "$cnf_path")
        local out_file="/tmp/solver_out_${core_id}.txt"
        local err_file="/tmp/solver_err_${core_id}.txt"

        # Run solver pinned to core
        local start_ns end_ns elapsed_ms exit_code
        start_ns=$(date +%s%N)
        set +e
        timeout "$TIMEOUT" taskset -c "$core_id" "$SOLVER" "$cnf_path" > "$out_file" 2> "$err_file"
        exit_code=$?
        set -e
        end_ns=$(date +%s%N)
        elapsed_ms=$(( (end_ns - start_ns) / 1000000 ))

        # Parse status
        local status="UNKNOWN"
        if [ $exit_code -eq 124 ]; then
            status="TIMEOUT"
        else
            local s_line
            s_line=$(grep "^s " "$out_file" 2>/dev/null || true)
            if echo "$s_line" | grep -q "SATISFIABLE"; then
                status="SATISFIABLE"
            elif echo "$s_line" | grep -q "UNSATISFIABLE"; then
                status="UNSATISFIABLE"
            fi
        fi

        # Parse CNF header for vars/clauses
        local num_vars=0 num_clauses=0
        local p_line
        p_line=$(grep "^p cnf" "$cnf_path" | head -1)
        if [ -n "$p_line" ]; then
            num_vars=$(echo "$p_line" | awk '{print $3}')
            num_clauses=$(echo "$p_line" | awk '{print $4}')
        fi

        # Parse SpinSAT stderr
        local restarts=0 method_used="null" strategy="null" zeta="null" seed="null"
        if [ -f "$err_file" ]; then
            local strat_line
            strat_line=$(grep "strategy=" "$err_file" 2>/dev/null || true)
            if [ -n "$strat_line" ]; then
                strategy=$(echo "$strat_line" | sed -n 's/.*strategy=\([^,]*\),.*/\1/p')
                zeta=$(echo "$strat_line" | sed -n 's/.*zeta=\([^,]*\),.*/\1/p')
                seed=$(echo "$strat_line" | sed -n 's/.*seed=\([0-9]*\).*/\1/p')
                [ -n "$strategy" ] && strategy="\"$strategy\"" || strategy="null"
                [ -n "$zeta" ] || zeta="null"
                [ -n "$seed" ] || seed="null"
            fi
            local solved_line
            solved_line=$(grep "Solved after" "$err_file" 2>/dev/null || true)
            if [ -n "$solved_line" ]; then
                restarts=$(echo "$solved_line" | sed -n 's/.*Solved after \([0-9]*\) restarts.*/\1/p')
                method_used=$(echo "$solved_line" | sed -n 's/.*using \([^ ]*\).*/\1/p')
                [ -n "$method_used" ] && method_used="\"$method_used\"" || method_used="null"
                [ -n "$restarts" ] || restarts=0
            fi
        fi

        local time_s
        time_s=$(awk "BEGIN {printf \"%.4f\", $elapsed_ms / 1000.0}")
        local ratio="0"
        if [ "$num_vars" -gt 0 ]; then
            ratio=$(awk "BEGIN {printf \"%.3f\", $num_clauses / $num_vars}")
        fi

        # Write per-instance result as JSON
        cat > "$RESULTS_TMP/${instance_name}.json" <<ENDJSON
{
  "instance": "$instance_name",
  "path": "$cnf_path",
  "spinsat": {
    "status": "$status",
    "time_s": $time_s,
    "verified": "N/A",
    "num_vars": $num_vars,
    "num_clauses": $num_clauses,
    "ratio": $ratio,
    "restarts": $restarts,
    "method_used": $method_used,
    "strategy": $strategy,
    "zeta": $zeta,
    "seed": $seed
  }
}
ENDJSON

        # Update progress (atomic via temp file)
        local completed
        completed=$(cat "$PROGRESS_FILE")
        completed=$((completed + 1))
        echo "$completed" > "$PROGRESS_FILE"

        local short_status="???"
        case "$status" in
            SATISFIABLE)   short_status="SAT" ;;
            UNSATISFIABLE) short_status="UNSAT" ;;
            TIMEOUT)       short_status="T/O" ;;
            UNKNOWN)       short_status="UNK" ;;
        esac
        echo "[$completed/$TOTAL] core=$core_id $instance_name → $short_status ${time_s}s"

    done < "$worker_queue"
}

# --- Distribute jobs round-robin to workers ---
for i in $(seq 0 $((PARALLELISM - 1))); do
    awk "NR % $PARALLELISM == $i" /tmp/job_queue.txt > "/tmp/worker_queue_${i}.txt"
done

for i in $(seq 0 $((PARALLELISM - 1))); do
    run_worker "$i" "/tmp/worker_queue_${i}.txt" &
done

echo "All $PARALLELISM workers launched. Waiting for completion..."
wait
echo "All workers finished."

# --- Merge per-instance JSONs into single results file ---
echo "Merging results..."

# Build JSON array from individual files
{
    echo "["
    first=true
    for f in "$RESULTS_TMP"/*.json; do
        [ -f "$f" ] || continue
        if [ "$first" = true ]; then
            first=false
        else
            echo ","
        fi
        cat "$f"
    done
    echo "]"
} > "$RESULTS_FILE"

echo "Results written to $RESULTS_FILE ($TOTAL instances)"

# Cleanup
rm -rf "$RESULTS_TMP" /tmp/job_queue.txt /tmp/worker_queue_*.txt "$PROGRESS_FILE"
