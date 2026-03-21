#!/usr/bin/env python3
"""Generate 20 benchmark instances for SAT Competition 2026 submission.

Generates planted CDC-style 3-SAT instances at various sizes and difficulties.
These are hard for local-search solvers but solvable by DMM.
"""
import os
import random
import sys

def gen_planted_3sat(n_vars, ratio, p0, seed):
    """Generate a planted-solution CDC-style 3-SAT instance.

    p0: probability all 3 literals are positive (controls difficulty).
    Lower p0 = harder (larger backbone). Paper uses p0=0.08 for hard instances.
    """
    random.seed(seed)
    n_clauses = int(n_vars * ratio)

    # Plant the all-true solution (will be gauge-transformed)
    solution = [True] * n_vars

    clauses = []
    for _ in range(n_clauses):
        # Pick 3 distinct variables
        vars_chosen = random.sample(range(n_vars), 3)

        # Assign polarities according to CDC distribution
        r = random.random()
        if r < p0:
            # All positive (probability p0)
            signs = [1, 1, 1]
        elif r < p0 + 3 * (1 - 4*p0) / 6:
            # One negative (probability 3*p1, choose which one)
            signs = [1, 1, 1]
            neg_idx = random.randint(0, 2)
            signs[neg_idx] = -1
        else:
            # Two negatives (probability 3*p2)
            signs = [-1, -1, -1]
            pos_idx = random.randint(0, 2)
            signs[pos_idx] = 1

        # Apply random gauge transformation (so solution is not trivially all-true)
        gauge = [random.choice([-1, 1]) for _ in range(3)]
        lits = []
        for j in range(3):
            var = vars_chosen[j] + 1  # 1-indexed
            pol = signs[j] * gauge[j]
            lits.append(pol * var)

        # Verify clause is satisfiable (at least one literal matches gauged solution)
        gauged_sol = [g == 1 for g in gauge]  # after gauge, solution becomes this
        sat = any(
            (lit > 0 and solution[abs(lit)-1]) or (lit < 0 and not solution[abs(lit)-1])
            for lit in lits
        )
        if not sat:
            # Fix by flipping one literal
            idx = random.randint(0, 2)
            lits[idx] = -lits[idx]

        clauses.append(lits)

    return n_vars, n_clauses, clauses

def write_cnf(filepath, n_vars, n_clauses, clauses, comment=""):
    with open(filepath, 'w') as f:
        f.write(f"c SpinSAT benchmark instance\n")
        f.write(f"c {comment}\n")
        f.write(f"p cnf {n_vars} {n_clauses}\n")
        for clause in clauses:
            f.write(" ".join(str(l) for l in clause) + " 0\n")

def main():
    outdir = sys.argv[1] if len(sys.argv) > 1 else "benchmarks/submission"
    os.makedirs(outdir, exist_ok=True)

    # 20 instances across difficulty levels
    configs = [
        # (n_vars, ratio, p0, count, description)
        (500,  4.3, 0.08, 4, "500v hard CDC"),
        (750,  4.3, 0.08, 4, "750v hard CDC"),
        (1000, 4.3, 0.08, 4, "1000v hard CDC"),
        (1500, 4.3, 0.08, 4, "1500v hard CDC"),
        (2000, 4.3, 0.08, 4, "2000v hard CDC"),
    ]

    idx = 1
    for n_vars, ratio, p0, count, desc in configs:
        for i in range(count):
            seed = 2026_000 + idx * 137
            n_vars_actual, n_clauses, clauses = gen_planted_3sat(n_vars, ratio, p0, seed)
            fname = f"spinsat-{idx:02d}-n{n_vars}-r{ratio}-p{p0}.cnf"
            fpath = os.path.join(outdir, fname)
            comment = f"{desc}, seed={seed}, p0={p0}, ratio={ratio}"
            write_cnf(fpath, n_vars_actual, n_clauses, clauses, comment)
            print(f"Generated {fname} ({n_vars}v, {n_clauses}c)")
            idx += 1

if __name__ == "__main__":
    main()
