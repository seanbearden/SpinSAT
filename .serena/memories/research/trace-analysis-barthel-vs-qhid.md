# Trace Analysis: barthel-320 vs qhid-320 Dynamics Comparison (2026-03-22)

## Setup
- barthel-320-1: 308 vars (after preprocess), α_r=4.3, solved in 0.3s
- qhid-320-1: 318 vars (after preprocess), α_r=5.5, solved in 15.8s (53x slower)

## Key Findings

### Flip Count
- barthel: 105,106 total flips (341/var mean)
- qhid: 921,849 total flips (2,899/var mean) — **8.8x more oscillation**

### Activity Spread
- barthel: max/min ratio 13.4x, CV=0.349
- qhid: max/min ratio 23.4x, CV=0.395 — more uneven, some vars locked while others thrash

### Convergence Pattern (flip rate by quintile)
- barthel: [14663, 20666, 21860, 23440, 24475] — increasing (never converges, just finds solution)
- qhid: [136065, 158554, 191868, 211196, 224165] — increasing even faster (fighting barrier harder over time)
- NEITHER shows declining flip rate — solver never enters convergence phase
- For qhid this lasts 12.8x longer (7464 vs 585 time-units)

### Variable Behavior
- qhid var 221, 268: ~336 flips (very stable — likely already at correct value)
- qhid var 160: 7867 flips (highly oscillating — trapped in deceptive basin)
- The stable vars (~30%) may be correct early; the oscillating ones fight the deceptive gradient

## Interpretation
The solver fights the exponential barrier via brute-force x_l memory accumulation.
The increasing flip rate = solver being pushed away from solution faster than memory compensates.
This is NOT convergence — it's escalating oscillation until memory forces overcome deception.

## Implication for Selective Restart ("Tunneling")
- Preserve low-flip-count variables (likely correct) during restart
- Only randomize high-flip-count variables (trapped in deceptive basin)
- This "tunnels" past the barrier by keeping ~30% correct assignments
- Detection signal: high flip rate + no unsat count improvement = deceptive barrier
