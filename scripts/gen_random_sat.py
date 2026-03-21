#!/usr/bin/env python3
"""Generate a random k-SAT instance (planted solution for guaranteed satisfiability)."""
import random
import sys

def gen_planted_3sat(n_vars, ratio, seed=None):
    """Generate a planted-solution 3-SAT instance."""
    if seed is not None:
        random.seed(seed)
    n_clauses = int(n_vars * ratio)
    # Plant a random solution
    solution = [random.choice([True, False]) for _ in range(n_vars)]
    clauses = []
    for _ in range(n_clauses):
        # Pick 3 distinct variables
        vars_chosen = random.sample(range(n_vars), 3)
        # Generate polarities ensuring at least one literal is satisfied
        lits = []
        for v in vars_chosen:
            if random.random() < 0.5:
                lits.append(v + 1)  # positive
            else:
                lits.append(-(v + 1))  # negative
        # Check if clause is satisfied by planted solution
        sat = any(
            (lit > 0 and solution[abs(lit)-1]) or (lit < 0 and not solution[abs(lit)-1])
            for lit in lits
        )
        if not sat:
            # Flip one literal to satisfy
            idx = random.randint(0, 2)
            lits[idx] = -lits[idx]
        clauses.append(lits)
    return n_vars, clauses

if __name__ == '__main__':
    n = int(sys.argv[1]) if len(sys.argv) > 1 else 1000
    r = float(sys.argv[2]) if len(sys.argv) > 2 else 4.3
    s = int(sys.argv[3]) if len(sys.argv) > 3 else 42
    n_vars, clauses = gen_planted_3sat(n, r, s)
    print(f"c Random planted 3-SAT: {n_vars} vars, {len(clauses)} clauses, ratio {r}")
    print(f"c seed={s}")
    print(f"p cnf {n_vars} {len(clauses)}")
    for c in clauses:
        print(" ".join(str(l) for l in c) + " 0")
