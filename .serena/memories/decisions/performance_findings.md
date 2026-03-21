# Performance Findings (2026-03-21, updated with deep research)

## Target Platform
Competition: Intel Xeon Platinum 8368 (Ice Lake), x86-64, Ubuntu 24.04
Local testing: Apple M-series (ARM64) — findings must generalize to x86-64

## Controlled A/B Testing Protocol
- Same instance, same seed (-s 1), same method (-m euler)
- Verify identical step counts to confirm same execution path
- Run 3x to confirm consistency (variance less than 1%)
- Use perf_compare.py for automated comparison

## Finding 1: Loop fission beats fusion — CONFIRMED UNIVERSAL
Splitting L-value computation from min-finding into two loops is 14% faster.

Root cause (from deep research):
- LLVM auto-vectorizer and SLP vectorizer operate at IR level (architecture-independent)
- Simple branch-free loop enables vectorization/full-unrolling
- Fusing with conditional min-finding prevents vectorization on ANY target
- Not Apple-specific — same LLVM behavior on x86-64

Action: Keep two-loop pattern. This is a compiler optimization principle, not a hardware quirk.

## Finding 2: AoS beats SoA at k=3 — CONFIRMED UNIVERSAL
Both Intel and Apple use 64-byte L1 cache lines.
3-literal clause = 48 bytes (3 x 16-byte pairs) fits in one cache line with AoS.
SoA requires two cache line fetches.

Crossover: SoA would win for k greater than 4 (L1) or k greater than 8 (with prefetching).
For 3-SAT (our primary target), AoS is unambiguously better.

Action: Keep Vec of Vec of (usize, f64) layout.

## Finding 3: u32 cast overhead — APPLE-SPECIFIC, does NOT apply to x86-64
On Apple M-series: mov w,w is not eliminated by register renamer.
On Intel/AMD: mov eax,eax zero-extension IS free (eliminated by renamer).
This finding would NOT transfer to the competition hardware.

Action: Keep usize anyway (AoS layout makes this moot). Do NOT optimize for Apple-specific behavior.

## Key Principle: Do NOT Optimize for Local Hardware
- All optimizations must be validated as architecture-general
- LLVM IR-level optimizations (loop structure, vectorization enablement) are universal
- Hardware-specific register allocation quirks are NOT portable
- When in doubt, optimize for the algorithm (fewer steps) not the constant factor

## Performance Profile
- Hot path: compute_derivatives, called once per integration step
- 2% of peak FLOP throughput — memory-bound, not compute-bound
- Bottleneck: random access to state.v[var_idx] (inherent to SAT structure)
- No micro-optimization will close the gap with CDCL solvers
- Algorithmic improvements (Trapezoid method, fewer steps) are highest value

## Remaining Optimization Opportunities (Priority Order)
1. Trapezoid vs Euler systematic benchmark — ALGORITHMIC (fewer steps)
2. Cache-friendly clause ordering — only matters at 100K+ variables
3. Cross-compile and benchmark on actual x86-64 hardware
