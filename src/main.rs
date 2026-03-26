use std::env;
use std::process;

use spinsat::dmm::Params;
use spinsat::parser;
use spinsat::preprocess;
use spinsat::solver::{solve, RestartMode, SolveResult, SolverConfig, Strategy};

fn main() {
    let args: Vec<String> = env::args().collect();

    let mut cnf_path: Option<String> = None;
    let mut timeout: f64 = 5000.0;
    let mut seed: u64 = 42;
    let mut zeta: Option<f64> = None;
    let mut auto_zeta = true;
    let mut strategy = Strategy::Adaptive;
    let mut do_preprocess = true;
    let mut cdcl_fallback = false;
    let mut proof_path: Option<String> = None;
    let mut detect_unsat = false;
    let mut no_restart = false;
    let mut restart_mode = RestartMode::Cycling;
    let mut restart_noise: f64 = 0.05;
    let mut xl_decay: f64 = 0.5;
    let mut use_sparse_engine = false;
    let mut cli_beta: Option<f64> = None;
    let mut cli_gamma: Option<f64> = None;
    let mut cli_delta: Option<f64> = None;
    let mut cli_epsilon: Option<f64> = None;
    let mut cli_alpha_initial: Option<f64> = None;
    let mut cli_alpha_up: Option<f64> = None;
    let mut cli_alpha_down: Option<f64> = None;
    let mut cli_alpha_interval: Option<f64> = None;
    #[cfg(feature = "trace")]
    let mut trace_mode: Option<String> = None;
    #[cfg(feature = "trace")]
    let mut trace_interval: u64 = 1000;
    #[cfg(feature = "trace")]
    let mut trace_output = String::from("trace.bin");
    #[cfg(feature = "trace")]
    let mut trace_memory = false;

    // Parse arguments
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--timeout" | "-t" => {
                i += 1;
                timeout = args.get(i).and_then(|s| s.parse().ok()).unwrap_or(5000.0);
            }
            "--seed" | "-s" => {
                i += 1;
                seed = args.get(i).and_then(|s| s.parse().ok()).unwrap_or(42);
            }
            "--zeta" | "-z" => {
                i += 1;
                zeta = args.get(i).and_then(|s| s.parse().ok());
                auto_zeta = false;
            }
            "--method" | "-m" => {
                i += 1;
                strategy = args
                    .get(i)
                    .and_then(|s| Strategy::from_str(s))
                    .unwrap_or_else(|| {
                        eprintln!(
                            "Invalid method. Use: euler, trapezoid, rk4, alternate, probe, auto"
                        );
                        process::exit(1);
                    });
            }
            "--no-auto-zeta" => {
                auto_zeta = false;
            }
            "--version" | "-V" => {
                println!("spinsat {}", env!("CARGO_PKG_VERSION"));
                process::exit(0);
            }
            "--no-preprocess" => {
                do_preprocess = false;
            }
            "--cdcl-fallback" => {
                cdcl_fallback = true;
            }
            "--proof" => {
                i += 1;
                proof_path = args.get(i).cloned();
            }
            "--detect-unsat" => {
                detect_unsat = true;
            }
            "--no-restart" => {
                no_restart = true;
            }
            "--restart-mode" | "-r" => {
                i += 1;
                restart_mode = args
                    .get(i)
                    .and_then(|s| RestartMode::from_str(s))
                    .unwrap_or_else(|| {
                        eprintln!("Invalid restart mode. Use: cold, warm, anti-phase, cycling");
                        process::exit(1);
                    });
            }
            "--restart-noise" => {
                i += 1;
                restart_noise = args.get(i).and_then(|s| s.parse().ok()).unwrap_or(0.1);
            }
            "--xl-decay" => {
                i += 1;
                xl_decay = args.get(i).and_then(|s| s.parse().ok()).unwrap_or(0.3);
            }
            "--beta" => {
                i += 1;
                cli_beta = args.get(i).and_then(|s| s.parse().ok());
            }
            "--gamma" => {
                i += 1;
                cli_gamma = args.get(i).and_then(|s| s.parse().ok());
            }
            "--delta" => {
                i += 1;
                cli_delta = args.get(i).and_then(|s| s.parse().ok());
            }
            "--epsilon" => {
                i += 1;
                cli_epsilon = args.get(i).and_then(|s| s.parse().ok());
            }
            "--alpha-initial" => {
                i += 1;
                cli_alpha_initial = args.get(i).and_then(|s| s.parse().ok());
            }
            "--alpha-up-mult" => {
                i += 1;
                cli_alpha_up = args.get(i).and_then(|s| s.parse().ok());
            }
            "--alpha-down-mult" => {
                i += 1;
                cli_alpha_down = args.get(i).and_then(|s| s.parse().ok());
            }
            "--alpha-interval" => {
                i += 1;
                cli_alpha_interval = args.get(i).and_then(|s| s.parse().ok());
            }
            "--sparse-engine" => {
                use_sparse_engine = true;
            }
            #[cfg(feature = "trace")]
            "--trace" => {
                i += 1;
                trace_mode = args.get(i).cloned();
            }
            #[cfg(feature = "trace")]
            "--trace-interval" => {
                i += 1;
                trace_interval = args.get(i).and_then(|s| s.parse().ok()).unwrap_or(1000);
            }
            #[cfg(feature = "trace")]
            "--trace-output" => {
                i += 1;
                trace_output = args.get(i).cloned().unwrap_or_else(|| "trace.bin".into());
            }
            #[cfg(feature = "trace")]
            "--trace-memory" => {
                trace_memory = true;
            }
            "--help" | "-h" => {
                eprintln!("Usage: spinsat [OPTIONS] <instance.cnf>");
                eprintln!();
                eprintln!("Options:");
                eprintln!("  -t, --timeout <secs>   Timeout in seconds (default: 5000)");
                eprintln!("  -s, --seed <n>         Initial random seed (default: 42)");
                eprintln!("  -m, --method <name>    Strategy: euler, trapezoid, rk4, alternate, probe, auto (default: auto)");
                eprintln!("  -z, --zeta <val>       Learning rate (default: auto by ratio)");
                eprintln!("      --no-auto-zeta     Disable auto zeta selection");
                eprintln!("      --no-preprocess    Skip CNF preprocessing");
                eprintln!("      --cdcl-fallback    Enable CaDiCaL CDCL fallback for UNSAT detection");
                eprintln!("      --proof <path>     Write DRAT proof to file (requires --cdcl-fallback)");
                eprintln!("      --detect-unsat     Enable UNSAT signal detection with CaDiCaL handoff");
                #[cfg(feature = "trace")]
                {
                    eprintln!("      --trace <mode>     Trace solution path: full or snapshot");
                    eprintln!("      --trace-interval N Steps between snapshots (default: 1000)");
                    eprintln!("      --trace-output <p> Trace output file (default: trace.bin)");
                    eprintln!("      --trace-memory     Also trace x_s and x_l memory variables");
                }
                eprintln!("  -r, --restart-mode <m>  Restart mode: cold, warm, anti-phase, cycling (default: cycling)");
                eprintln!("      --restart-noise <v> Noise scale for warm/anti-phase restarts (default: 0.05)");
                eprintln!("      --xl-decay <v>      x_l decay factor for warm restarts (default: 0.5)");
                eprintln!("      --no-restart       Disable restarts (single continuous integration run)");
                eprintln!("      --beta <val>       Short-term memory growth rate (default: 20)");
                eprintln!("      --gamma <val>      Short-term memory threshold (default: 0.25)");
                eprintln!("      --delta <val>      Long-term memory threshold (default: 0.05)");
                eprintln!("      --epsilon <val>    Trapping rate (default: 1e-3)");
                eprintln!("      --alpha-initial <v> Initial per-clause alpha_m (default: 5)");
                eprintln!("      --alpha-up-mult <v> Alpha increase multiplier (default: 1.1)");
                eprintln!("      --alpha-down-mult <v> Alpha decrease multiplier (default: 0.9)");
                eprintln!("      --alpha-interval <v> Time between alpha adjustments (default: 1e4)");
                eprintln!("      --sparse-engine    Use sparse matrix derivative engine (challenger)");
                eprintln!("  -V, --version          Print version");
                eprintln!("  -h, --help             Show this help");
                process::exit(0);
            }
            arg if !arg.starts_with('-') => {
                cnf_path = Some(arg.to_string());
            }
            _ => {
                eprintln!("Unknown option: {}", args[i]);
                process::exit(1);
            }
        }
        i += 1;
    }

    let cnf_path = match cnf_path {
        Some(p) => p,
        None => {
            eprintln!("Usage: spinsat [OPTIONS] <instance.cnf>");
            process::exit(1);
        }
    };

    // Parse DIMACS CNF
    let raw_formula = match parser::parse_dimacs(&cnf_path) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("c Error parsing {}: {}", cnf_path, e);
            println!("s UNKNOWN");
            process::exit(0);
        }
    };

    let original_num_vars = raw_formula.num_vars;
    let original_num_clauses = raw_formula.num_clauses();

    eprintln!("c SpinSAT v{} — DMM-based SAT solver", env!("CARGO_PKG_VERSION"));
    eprintln!(
        "c Instance: {} variables, {} clauses (ratio {:.2})",
        original_num_vars,
        original_num_clauses,
        original_num_clauses as f64 / original_num_vars as f64,
    );

    // Preprocess and build formula
    use spinsat::formula::Formula;
    let raw_clauses = raw_formula.into_raw_clauses();

    let (formula, preprocess_result) = if do_preprocess {
        let result = preprocess::preprocess(original_num_vars, raw_clauses);

        eprintln!(
            "c Preprocessing: {} vars → {}, {} clauses → {} (eliminated {} vars, {} clauses)",
            original_num_vars,
            result.num_vars,
            original_num_clauses,
            result.clauses.len(),
            result.stats.vars_eliminated,
            result.stats.clauses_eliminated,
        );
        eprintln!(
            "c   unit_prop={}, pure_lit={}, subsump={}, self_sub={}, bve={}, probe={}",
            result.stats.unit_props,
            result.stats.pure_literals,
            result.stats.subsumptions,
            result.stats.self_subsumptions,
            result.stats.bve_eliminations,
            result.stats.failed_literals,
        );

        if result.num_vars == 0 {
            // Fully solved by preprocessing
            eprintln!("c Solved by preprocessing alone!");
            let full_assignment = result.reconstruct_assignment(&[], original_num_vars);
            println!("s SATISFIABLE");
            print_assignment(&full_assignment);
            process::exit(0);
        }

        let formula = Formula::new(result.num_vars, result.clauses.clone());
        (formula, Some(result))
    } else {
        (Formula::new(original_num_vars, raw_clauses), None)
    };
    let mut formula = formula;

    let ratio = formula.num_clauses() as f64 / formula.num_vars as f64;

    // Set parameters
    let mut params = Params::default();
    if let Some(v) = cli_beta { params.beta = v; }
    if let Some(v) = cli_gamma { params.gamma = v; }
    if let Some(v) = cli_delta { params.delta = v; }
    if let Some(v) = cli_epsilon { params.epsilon = v; }
    if let Some(v) = cli_alpha_initial { params.alpha_initial = v; }
    if let Some(v) = cli_alpha_up { params.alpha_up = v; }
    if let Some(v) = cli_alpha_down { params.alpha_down = v; }
    if let Some(v) = cli_alpha_interval { params.alpha_interval = v; }
    if auto_zeta {
        params = params.with_auto_zeta(ratio);
    }
    if let Some(z) = zeta {
        params.zeta = z;
    }

    eprintln!(
        "c Parameters: strategy={:?}, restart_mode={:?}, zeta={:.0e}, seed={}",
        strategy, restart_mode, params.zeta, seed
    );
    eprintln!(
        "c ODE params: beta={}, gamma={}, delta={}, epsilon={:.0e}",
        params.beta, params.gamma, params.delta, params.epsilon
    );
    eprintln!(
        "c Alpha params: initial={}, up_mult={}, down_mult={}, interval={}",
        params.alpha_initial, params.alpha_up, params.alpha_down, params.alpha_interval
    );

    // Configure solver
    let mut config = SolverConfig {
        timeout_secs: timeout,
        initial_seed: seed,
        strategy,
        restart_mode,
        restart_noise,
        xl_decay,
        cdcl_fallback,
        proof_path,
        enable_unsat_detection: detect_unsat,
        #[cfg(feature = "trace")]
        trace_config: trace_mode.map(|mode| {
            use spinsat::trace::{TraceConfig, TraceMode};
            let mode = match mode.as_str() {
                "full" => TraceMode::Full,
                "snapshot" => TraceMode::Snapshot { interval_steps: trace_interval },
                _ => {
                    eprintln!("Invalid trace mode '{}'. Use: full, snapshot", mode);
                    process::exit(1);
                }
            };
            TraceConfig {
                mode,
                output_path: trace_output.clone(),
                trace_memory,
            }
        }),
        ..Default::default()
    };

    config.use_sparse_engine = use_sparse_engine;

    if no_restart {
        config.max_restarts = u32::MAX;
        config.stagnation_patience = u32::MAX; // Never stagnate — run continuously
    }

    // Solve
    match solve(&mut formula, &params, &config) {
        SolveResult::Sat(reduced_assignment) => {
            let full_assignment = if let Some(ref pp) = preprocess_result {
                pp.reconstruct_assignment(&reduced_assignment, original_num_vars)
            } else {
                reduced_assignment
            };
            println!("s SATISFIABLE");
            print_assignment(&full_assignment);
        }
        SolveResult::Unsat => {
            println!("s UNSATISFIABLE");
        }
        SolveResult::Unknown | _ => {
            println!("s UNKNOWN");
        }
    }
}

/// Print assignment in SAT competition format.
fn print_assignment(assignment: &[bool]) {
    let mut line = String::from("v ");
    for (i, &val) in assignment.iter().enumerate() {
        let var_num = (i + 1) as i32;
        let lit = if val { var_num } else { -var_num };

        let token = format!("{}", lit);
        if line.len() + token.len() + 2 > 4090 {
            line.push('0');
            println!("{}", line);
            line = String::from("v ");
        }
        line.push_str(&token);
        line.push(' ');
    }
    line.push('0');
    println!("{}", line);
}
