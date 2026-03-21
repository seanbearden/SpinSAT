# Hybrid Restart Strategy Benchmark Results (2026-03-21)

## Strategies Tested
- euler: Fixed Euler only
- trapezoid: Fixed Trapezoid only
- alternate: Euler/Trap alternating on restart
- probe: Test both, commit to winner
- auto (adaptive): Track effectiveness, bias toward winner

## Result 1: All instances (86 planted, seed=1, 120s timeout)
euler=414.0  alternate=414.4  auto=419.8  probe=597.2  trapezoid=1131.5
Trapezoid-only has 4 timeouts. All others solve 86/86.

## Result 2: CDC submission instances (20, 500-2000 vars, 300s timeout)
euler=337.9  alternate=335.5  auto=337.9
All identical within 0.7%.

## Result 3: Hard CDC instances (8, 3000-5000 vars, 500s timeout)
euler=733.1  auto=734.6  alternate=735.6
All identical within 0.3%. No restarts triggered.

## Conclusion
The hybrid strategies do NOT improve over Euler-only on our test instances.
Reason: instances either solve on first attempt (no restarts needed) or
the restart trigger fires at the same point regardless of method.

The strategies only diverge when:
- An instance triggers multiple restarts AND
- Euler and Trapezoid have different convergence behavior on that specific instance

With seed=1 and our current instances, this doesn't happen enough to matter.

## Decision
Keep auto (adaptive) as default — it never regresses and is future-proof.
But Euler-only is equally good for current benchmarks.
The real performance gains will come from algorithmic improvements
(better restart criteria, modified equations) not method switching.
