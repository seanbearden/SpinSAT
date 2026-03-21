# SAT Competition 2026 Submission Requirements -- Research Report

**Research Date**: 2026-03-20
**Confidence Level**: HIGH (90%+) -- based on direct extraction of official competition pages
**Sources**: satcompetition.github.io/2026, /2025, /2024, SoSy-Lab competition-scripts GitLab, Nature, arXiv, SAT heritage project

---

## 1. Submission Format

### 2026 (Current Year -- Call for Solvers Sent 2026-03-17)

**Sequential/Main Track**: Source code submitted via **private GitHub repository**.
- After the submission deadline, repositories must be made **public**.
- No binary-only submissions for Main Track (source code is mandatory).

**Parallel Track**: Source code submitted via **GitHub repository** with Dockerfile.
- Uses AWS infrastructure: https://github.com/aws-samples/aws-batch-comp-infrastructure-sample
- Must provide a Dockerfile that builds the solver image.

**What you submit**:
1. A GitHub repository containing:
   - Source code
   - `build.sh` script (takes no parameters, builds the solver)
   - `run.sh` script (takes two parameters: path to benchmark, path to proof output directory)
2. System description document (1-2 pages, IEEE Proceedings style, PDF)
3. 20 benchmark instances (for Main Track participants)
4. Registration email to organizers@satcompetition.org

### Historical Context
- **2024**: Used StarExec platform -- uploaded zip files with source + build script
- **2025**: Switched to BenchCloud (SoSy Lab Munich) -- GitHub repository submission
- **2026**: Uses HoreKa Blue (NHR@KIT Karlsruhe) for sequential, AWS for parallel

---

## 2. Build Requirements

### Build System (2025-2026)

The solver must be buildable and runnable on a system conforming to the **SoSy Lab Competition Scripts computing environment**.

**Two required scripts in the top-level directory:**

```bash
# build.sh -- builds the solver, takes no parameters
#!/bin/bash
# Your build commands here (make, cmake, cargo, etc.)

# run.sh -- runs the solver
#!/bin/bash
# $1 = path to benchmark instance (DIMACS CNF file)
# $2 = path to output directory (write proof.out here for UNSAT)
./solver $1 --proof-file=$2/proof.out
```

**Docker image for testing**: `registry.gitlab.com/sosy-lab/benchmarking/competition-scripts/user:latest`

Test your solver:
```bash
podman pull registry.gitlab.com/sosy-lab/benchmarking/competition-scripts/user:latest
podman run --rm -i -t --volume=<TOOL>:/tool --workdir=/tool \
  registry.gitlab.com/sosy-lab/benchmarking/competition-scripts/user:latest bash
./build.sh
./run.sh /path/to/instance.cnf /path/to/output/
```

### Compilers and Tools Available (from Dockerfile.user.2026)

The competition Docker image is based on **Ubuntu 24.04** with the following installed:

| Category | Packages |
|----------|----------|
| **C/C++ Compilers** | `gcc`, `g++-multilib`, `clang`, `clang-14` |
| **Build Tools** | `make` (CMake NOT explicitly listed but commonly available) |
| **LLVM** | `llvm`, `llvm-14`, `llvm-dev`, `libclang-cpp-dev`, `libclang-dev` |
| **Java** | OpenJDK 8, 11, 17, 21 (JRE and JDK) |
| **Python** | `python3` + libraries (bitstring, clang, docker, graphviz, jsonschema, lxml, pycparser, requests, tqdm, yaml) |
| **Math Libraries** | `libgmp-dev`, `libmpfr-dev`, `libmpfr6`, `libflint-dev` |
| **Other** | `zlib1g-dev`, `lcov`, `clang-format`, `clang-tidy`, `libgomp1` |

### What's NOT pre-installed (you must include in your repo or build.sh)
- **Rust/Cargo** -- not in the Docker image
- **CMake** -- not explicitly listed (but may be available via system packages)
- **Boost** -- not listed
- Any custom libraries

**Key implication**: Your `build.sh` can install additional packages via `apt-get` if needed, since you have the full Ubuntu 24.04 base. However, for reliability, it is strongly recommended to stick to what is already installed or bundle dependencies.

---

## 3. Language Restrictions

**There are NO explicit language restrictions.** The competition does not mandate any specific programming language. Your solver needs to:

1. Be compilable/runnable via `build.sh` and `run.sh`
2. Run within the Docker environment (Ubuntu 24.04)
3. Read DIMACS CNF from a file path
4. Write output to stdout and proofs to a file

### Practical Language Analysis

| Language | Feasibility | Notes |
|----------|------------|-------|
| **C/C++** | Excellent | GCC and Clang pre-installed. Virtually all competitive solvers use C/C++. |
| **Python** | Possible but risky | Python3 is installed. Performance will be a major concern given 5000s time limit and competition-scale instances (100K+ variables). |
| **Rust** | Possible with effort | NOT pre-installed. Your `build.sh` would need to install rustup/cargo first, or you could pre-compile and include the binary. |
| **Java** | Possible | JDK 8/11/17/21 installed. Performance overhead from JVM may be significant. |

**Reality check**: Every competitive solver in the history of SAT competitions has been written in C or C++. The top solvers (Kissat, CaDiCaL, MiniSat, Glucose) are all C/C++. A Python solver would be at an extreme performance disadvantage but is technically allowed.

**For a DE/ODE-based solver**: C/C++ is strongly recommended. The ODE integration is compute-intensive and Python would be orders of magnitude too slow for competition-scale instances.

---

## 4. Runtime Environment

### Main Track (Sequential) -- 2026

| Parameter | Value |
|-----------|-------|
| **Hardware** | HoreKa Blue, NHR@KIT Karlsruhe |
| **CPU** | Intel Xeon Platinum 8368 (76 cores per node, but 8 benchmarks run in parallel) |
| **Effective cores per solver** | ~9-10 cores available, but **single-threaded execution expected** |
| **Time limit** | **5000 seconds** (CPU time) |
| **Memory limit** | **32 GB** |
| **Operating System** | Ubuntu 24.04 (containerized) |
| **Ranking** | PAR-2 score (runtime + 2x timeout for unsolved) |
| **Benchmarks** | 300-600 instances |

### Parallel Track -- 2026

| Parameter | Value |
|-----------|-------|
| **Platform** | Amazon Web Services (AWS) |
| **Instance type** | m6i.16xlarge |
| **vCPUs** | 64 virtual cores |
| **Memory** | 256 GB |
| **Time limit** | **1000 seconds** (wall-clock time) |
| **Submission** | Dockerfile via GitHub |

### Experimental Track -- 2026 (NEW)

- For solvers using techniques **not yet supported by certificate generation**
- Evaluated only on NEW benchmark instances
- Must outperform top 3 Main Track solvers on those instances to receive an award
- **This is the most viable track for a DE/ODE-based solver** since it does not require UNSAT proof certificates

### Cloud Track -- 2026
- TBA (not yet defined)

### Historical Comparison

| Year | Platform | Time Limit | Memory Limit |
|------|----------|-----------|--------------|
| 2024 | StarExec | 5000s | 128 GB |
| 2025 | BenchCloud (LMU Munich) | 5000s | 30 GB |
| 2026 | HoreKa Blue (KIT) | 5000s | 32 GB |

---

## 5. Input/Output Format

### Input: DIMACS CNF Format

```
c This is a comment line
c Another comment
p cnf 5 3
1 -5 4 0
-1 5 3 4 0
-3 -4 0
```

**Specification**:
- Lines starting with `c` are comments (optional)
- Header line: `p cnf <num_variables> <num_clauses>`
- `<num_variables>`: exact number of variables appearing in the file
- `<num_clauses>`: exact number of clauses
- Each clause: space-separated list of non-null integers in range [-nbvar, nbvar], terminated by `0`
- Positive integer = variable; negative integer = negation of variable
- A clause must NOT contain both i and -i simultaneously
- All literals in a clause must be distinct

### Output Format (to stdout)

**Three types of lines allowed**:

1. **Comment lines**: `c <text>` (optional, can appear anywhere)
2. **Solution line** (MANDATORY, exactly once):
   - `s SATISFIABLE` -- found a model
   - `s UNSATISFIABLE` -- proved no model exists
   - `s UNKNOWN` -- could not determine
3. **Value lines** (required if SAT): `v <literals> ... 0`
   - Space-separated list of non-contradictory literals
   - Partial assignments allowed (at least one literal per clause must appear)
   - Maximum 4096 characters per value line
   - Last value line terminated by `0`

**Example SAT output**:
```
c My solver v1.0
s SATISFIABLE
v 1 -2 3 -4 5 0
```

**Example UNSAT output**:
```
c My solver v1.0
s UNSATISFIABLE
```
(Plus proof written to `$2/proof.out`)

### Exit Codes (for parallel/cloud track)
- **10**: Satisfiable
- **20**: Unsatisfiable
- Anything else: Error

### UNSAT Proof Certificates (Main Track ONLY)

**Required for Main Track**. Must be written to file `proof.out` in the directory given as `$2` to `run.sh`.

Three verified proof checker options:
1. **DPR/LRAT** (cake_lpr) -- textual or binary format
2. **GRAT** -- textual or binary format
3. **VeriPB** (CakePB) -- pseudo-Boolean proof format

**For the Experimental Track**: UNSAT proof certificates are NOT required. This is the key distinction.

---

## 6. Solver Categories / Tracks (2026)

### Main Track
- Sequential SAT solvers
- Must provide SAT models AND UNSAT proofs
- Must submit 20 new benchmarks
- Source code must be open (licensed for research)
- Max 4 solvers per participant team
- PAR-2 ranking

### Experimental Track (NEW in 2026)
- For solvers with techniques not supported by certificate generation
- Evaluated on NEW benchmarks only
- Must outperform top 3 Main Track solvers on new benchmarks for award
- No UNSAT proof requirement
- **Best fit for unconventional approaches (DE/ODE solvers)**

### AI-Generated / AI-Tuned Sub-Tracks (NEW in 2026)
- Separate subcategories in each track (Main, Experimental, Parallel, Cloud)
- Same rules apply but separate prizes
- Not eligible for regular track prizes
- Award for best AI-tuned and AI-generated solver in each track

### Parallel Track
- Multi-threaded solvers on AWS (m6i.16xlarge, 64 vCPUs, 256GB)
- 1000s wall-clock timeout
- Must provide SAT models
- Max 2 solvers per participant
- Dockerfile submission

### Cloud Track
- TBA for 2026

### CaDiCaL Hack Track
- Not mentioned for 2026 (was present in 2024-2025)

---

## 7. Licensing Requirements

**Main Track and Experimental Track**:
- Source code **must** be made available
- Must be **licensed for research purposes**
- After submission deadline, GitHub repositories must be made **public**

**No-limits Track** (existed in 2024-2025, not explicitly in 2026):
- Exception to open source requirement
- Binary-only submissions were allowed

**Practical implication**: You must be willing to open-source your solver. The license can be restrictive (research-only is fine), but the code must be publicly accessible after the competition.

---

## 8. Composition Rules

**Pure portfolios are BANNED**: You cannot combine multiple SAT solvers from different author groups.

**Allowed compositions**: Solvers combining different methodologies (CDCL, SLS, Lookahead, Groebner Basis, etc.). The rules explicitly list "Groebner Basis" as an example of a valid methodology -- this suggests **unconventional approaches are welcome** as long as they represent a distinct solving methodology.

**Exception**: If you write the entire solver yourself, composition restrictions do not apply.

**Implication for DE/ODE solver**: A continuous dynamical system approach would clearly constitute a distinct solving methodology. It could also be composed with CDCL techniques (e.g., using DE as a preprocessor or for initial variable phase selection).

---

## 9. StarExec Platform (Historical)

StarExec was used through 2024. Key characteristics:
- Cluster at University of Iowa
- Linux-based compute nodes
- 128 GB RAM per node (2024)
- Upload zip with source + build script
- Pre-defined directory structure required
- `starexec_run_default` script served as entry point

**No longer used**: 2025 switched to BenchCloud (LMU Munich), 2026 uses HoreKa Blue (NHR@KIT). The StarExec constraints are historical and no longer relevant.

---

## 10. Physics-Inspired / ODE-Based Solver History

### CTDS (Continuous-Time Dynamical System) -- Toroczkai et al.

**Key publications**:
- Ercsey-Ravasz & Toroczkai (2011): Original analog SAT solver based on coupled ODEs
- Molnar & Ercsey-Ravasz (2013): Asymmetric continuous-time neural networks for CSP
- Ercsey-Ravasz & Toroczkai (2018, Nature Comms): Extension to MaxSAT (Max-CTDS)
- Yin, Toroczkai, Hu (2019): AC-SAT analog hardware implementation (ASIC)
- Zheng et al. (2020, Computer Physics Comms): GPU-accelerated CTDS

### Performance Claims vs. Reality

**Claims**:
- "Polynomial analog time-complexity on hardest k-SAT"
- GPU implementation "several orders of magnitude" faster than CPU, "up to 100x faster than MiniSat"
- AC-SAT hardware: "10^5 to 10^6 speedup over software CTDS, 10^4 over MiniSat"

**Critical limitations that affect competition viability**:

1. **Scalability wall**: The CTDS papers test on **small instances** (N=10 to N=300 variables, ~1000 clauses). Competition instances have **100,000+ variables and millions of clauses**. The ODE system has O(N+M) equations -- for a 100K variable, 500K clause instance, you would integrate 600,000 coupled ODEs.

2. **Auxiliary variable growth**: The auxiliary variables `a_m(t)` can **grow exponentially** for hard instances. This is fundamental to the approach -- the exponential growth is what allows the solver to escape local optima, but it creates severe numerical challenges (stiffness, overflow).

3. **Stiffness**: For hard problems, the ODEs become extremely stiff, requiring very small time steps and making integration prohibitively slow. The 2018 MaxSAT paper explicitly notes stiffness issues.

4. **Incomplete solver**: CTDS is fundamentally an **incomplete SAT solver** -- it cannot prove UNSAT. It minimizes unsatisfied clauses but has no mechanism for generating UNSAT proofs. This means:
   - **Cannot participate in Main Track** (which requires UNSAT proofs)
   - **Can participate in Experimental Track** (no proof requirement)
   - Will return `s UNKNOWN` for UNSAT instances (scoring 2x timeout under PAR-2)

5. **No competition submissions found**: Despite extensive searching, I found **no evidence** that a CTDS/analog/ODE-based solver has ever been submitted to the SAT Competition. The academic papers benchmark against competition instances offline but have never entered the competition itself.

### Related Work: FastFourierSAT (2023-2024)

A more recent approach by Zheng et al. uses **continuous local search (CLS)** with Fourier expansion and GPU parallelism:
- Uses gradient-based optimization in a continuous relaxation
- GPU-accelerated (100x+ faster than CPU CLS)
- Tested on SAT Competition 2023 instances
- Also an **incomplete solver** (cannot prove UNSAT)
- Shows promise on SAT instances but struggles with large industrial UNSAT instances

### cBRIM-TMB (2023, published at SAT 2023 conference)

Combines continuous dynamical system (CDS) with make/break heuristics:
- Hybrid approach: CDS dynamics + discrete local search heuristics
- Claims "orders of magnitude faster than software SAT solvers"
- Tested on random and scale-free instances
- Paper acknowledges: "Testing on real SAT instances is also needed" and "real SAT instances are huge (hundreds of thousands of variables), efficient software-hardware co-design approach is necessary"
- Again, never entered the actual SAT Competition

---

## Summary: What Can and Cannot Be Built

### CAN do:
- Submit a solver written in any language (C/C++ strongly recommended)
- Use any solving methodology including ODE/DE integration
- Target the **Experimental Track** (no UNSAT proof requirement)
- Use the AI-Generated/AI-Tuned sub-track if applicable
- Compose a DE solver with conventional techniques
- Include custom libraries in your repository
- Install additional packages via build.sh

### CANNOT do (or severe challenges):
- **Main Track without UNSAT proofs**: DE solvers cannot prove UNSAT
- **Compete on UNSAT instances**: Will always return UNKNOWN, getting 2x timeout penalty
- **Use Python for the core solver**: Too slow for competition-scale instances
- **Ignore scalability**: Competition instances are 100-1000x larger than anything tested in CTDS papers
- **Avoid numerical issues**: Stiff ODEs and exponential auxiliary growth are fundamental challenges

### Recommended Strategy:
1. **Target Experimental Track** -- designed for unconventional approaches
2. **Write core solver in C/C++** for performance
3. **Use adaptive ODE integrator** (e.g., Dormand-Prince/RK45) with stiffness handling
4. **Focus on SAT instances** -- the solver will inherently only solve satisfiable instances
5. **Consider hybrid approach** -- use DE dynamics for variable phase initialization, combined with lightweight local search for final solution polishing
6. **Benchmark against competition instances** from prior years (available for download) to understand scalability limits
7. **Submit 20 benchmarks** -- required for Main Track, may be required for Experimental too

### Key Deadlines (2026):
- **Solver Registration + Benchmarks**: April 19th
- **Solver Submission**: April 26th
- **Solver Documentation**: May 17th

---

## Sources

- [SAT Competition 2026 Main Page](https://satcompetition.github.io/2026/)
- [SAT Competition 2026 Tracks](https://satcompetition.github.io/2026/tracks.html)
- [SAT Competition 2026 Rules](https://satcompetition.github.io/2026/rules.html)
- [SAT Competition 2026 Output Format](https://satcompetition.github.io/2026/output.html)
- [SAT Competition 2026 HoreKa Blue](https://satcompetition.github.io/2026/nhr.html)
- [SAT Competition 2026 AWS](https://satcompetition.github.io/2026/aws.html)
- [SAT Competition 2026 Benchmarks](https://satcompetition.github.io/2026/benchmarks.html)
- [SAT Competition 2025 Main Page](https://satcompetition.github.io/2025/)
- [SAT Competition 2025 Rules](https://satcompetition.github.io/2025/rules.html)
- [SAT Competition 2025 Tracks](https://satcompetition.github.io/2025/tracks.html)
- [SAT Competition 2025 BenchCloud](https://satcompetition.github.io/2025/benchcloud.html)
- [SAT Competition 2025 Results Slides (PDF)](https://satcompetition.github.io/2025/satcomp25slides.pdf)
- [SAT Competition 2024 Main Page](https://satcompetition.github.io/2024/)
- [SAT Competition 2024 Rules](https://satcompetition.github.io/2024/rules.html)
- [SAT Competition 2024 Tracks](https://satcompetition.github.io/2024/tracks.html)
- [SAT Competition 2024 StarExec](https://satcompetition.github.io/2024/starexec.html)
- [SoSy Lab Competition Scripts (Dockerfile)](https://gitlab.com/sosy-lab/benchmarking/competition-scripts/-/blob/main/test/Dockerfile.user.2026)
- [SoSy Lab Competition Scripts README](https://gitlab.com/sosy-lab/benchmarking/competition-scripts/-/blob/main/README.md)
- [CTDS MaxSAT (Nature Comms 2018)](https://www.nature.com/articles/s41467-018-07327-2)
- [GPU-accelerated CTDS (Computer Physics Comms 2020)](https://www.sciencedirect.com/science/article/abs/pii/S0010465520302204)
- [AC-SAT Hardware (NSF 2019)](https://par.nsf.gov/servlets/purl/10100229)
- [cBRIM-TMB (SAT 2023)](https://drops.dagstuhl.de/storage/00lipics/lipics-vol271-sat2023/LIPIcs.SAT.2023.25/LIPIcs.SAT.2023.25.pdf)
- [SPICE analog SAT modeling (arXiv 2024)](https://arxiv.org/html/2412.14690v1)
- [SAT Heritage Docker Images](https://github.com/sat-heritage/docker-images)
