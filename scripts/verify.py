#!/usr/bin/env python3
"""Independent SAT solution verifier. Checks solver output against CNF file."""

import sys
import subprocess

def parse_cnf(path):
    """Parse DIMACS CNF, return (num_vars, list of clauses)."""
    clauses = []
    num_vars = 0
    current = []
    with open(path) as f:
        for line in f:
            line = line.strip()
            if not line or line.startswith('c') or line.startswith('%'):
                continue
            if line.startswith('p cnf'):
                parts = line.split()
                num_vars = int(parts[2])
                continue
            for tok in line.split():
                lit = int(tok)
                if lit == 0:
                    if current:
                        clauses.append(current)
                        current = []
                else:
                    current.append(lit)
    if current:
        clauses.append(current)
    return num_vars, clauses

def parse_solver_output(text):
    """Parse solver stdout, return (status, assignment_dict or None)."""
    status = None
    lits = []
    for line in text.splitlines():
        line = line.strip()
        if line.startswith('s '):
            status = line[2:]
        elif line.startswith('v '):
            for tok in line[2:].split():
                val = int(tok)
                if val != 0:
                    lits.append(val)
    if status != 'SATISFIABLE':
        return status, None
    assignment = {}
    for lit in lits:
        var = abs(lit)
        assignment[var] = lit > 0
    return status, assignment

def verify(clauses, assignment):
    """Check every clause is satisfied."""
    for i, clause in enumerate(clauses):
        satisfied = False
        for lit in clause:
            var = abs(lit)
            val = assignment.get(var, True)  # default True if not assigned
            if (lit > 0 and val) or (lit < 0 and not val):
                satisfied = True
                break
        if not satisfied:
            return False, i
    return True, -1

def main():
    if len(sys.argv) < 3:
        print("Usage: verify.py <solver_binary> <cnf_file> [cnf_file2 ...]")
        sys.exit(1)

    solver = sys.argv[1]
    cnf_files = sys.argv[2:]

    total = 0
    passed = 0
    failed = 0

    for cnf_path in cnf_files:
        total += 1
        num_vars, clauses = parse_cnf(cnf_path)

        # Run solver
        result = subprocess.run([solver, cnf_path], capture_output=True, text=True, timeout=300)
        status, assignment = parse_solver_output(result.stdout)

        if status != 'SATISFIABLE':
            print(f"SKIP {cnf_path}: solver returned {status}")
            continue

        if assignment is None:
            print(f"FAIL {cnf_path}: no assignment in output")
            failed += 1
            continue

        ok, bad_clause = verify(clauses, assignment)
        if ok:
            print(f"OK   {cnf_path} ({num_vars} vars, {len(clauses)} clauses)")
            passed += 1
        else:
            print(f"FAIL {cnf_path}: clause {bad_clause} not satisfied: {clauses[bad_clause]}")
            failed += 1

    print(f"\nResults: {passed}/{total} verified, {failed} FAILED")
    return 1 if failed > 0 else 0

if __name__ == '__main__':
    sys.exit(main())
