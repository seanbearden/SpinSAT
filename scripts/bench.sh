#!/bin/bash
# Benchmark and verify SpinSAT on a set of CNF files.
# Usage: bench.sh <timeout_secs> <cnf_file1> [cnf_file2 ...]

SOLVER="./target/release/spinsat"
CHECKER="./scripts/check_sat"
TIMEOUT=${1:-60}
shift

SOLVED=0
FAILED=0
TIMEOUT_COUNT=0
TOTAL=0

for CNF in "$@"; do
    TOTAL=$((TOTAL+1))
    TMPOUT=$(mktemp)

    START=$(python3 -c "import time; print(time.time())")
    gtimeout "$TIMEOUT" "$SOLVER" "$CNF" > "$TMPOUT" 2>/dev/null
    EXIT=$?
    END=$(python3 -c "import time; print(time.time())")
    ELAPSED=$(python3 -c "print(f'{$END - $START:.2f}')")

    if [ $EXIT -eq 124 ]; then
        TIMEOUT_COUNT=$((TIMEOUT_COUNT+1))
        printf "TIMEOUT  %6ss  %s\n" "$TIMEOUT" "$(basename "$CNF")"
    else
        STATUS=$(grep "^s " "$TMPOUT" | awk '{print $2}')
        if [ "$STATUS" = "SATISFIABLE" ]; then
            RESULT=$("$CHECKER" "$CNF" "$TMPOUT" 2>/dev/null)
            if echo "$RESULT" | grep -q "^OK"; then
                SOLVED=$((SOLVED+1))
                printf "OK       %6ss  %s\n" "$ELAPSED" "$(basename "$CNF")"
            else
                FAILED=$((FAILED+1))
                printf "WRONG    %6ss  %s\n" "$ELAPSED" "$(basename "$CNF")"
            fi
        else
            printf "UNKNOWN  %6ss  %s\n" "$ELAPSED" "$(basename "$CNF")"
        fi
    fi
    rm -f "$TMPOUT"
done

echo ""
echo "=== RESULTS ==="
echo "Solved+Verified: $SOLVED / $TOTAL"
echo "Timeouts: $TIMEOUT_COUNT"
echo "Wrong answers: $FAILED"
