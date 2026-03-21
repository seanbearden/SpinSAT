mod dmm;
mod formula;
mod integrator;
mod parser;
mod solver;

use std::env;
use std::process;

use dmm::Params;
use integrator::Method;
use solver::{solve, SolveResult, SolverConfig};

fn main() {
    let args: Vec<String> = env::args().collect();

    let mut cnf_path: Option<String> = None;
    let mut timeout: f64 = 5000.0;
    let mut seed: u64 = 42;
    let mut zeta: Option<f64> = None;
    let mut auto_zeta = true;
    let mut method = Method::Euler;

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
                method = args
                    .get(i)
                    .and_then(|s| Method::from_str(s))
                    .unwrap_or_else(|| {
                        eprintln!("Invalid method. Use: euler, trapezoid, rk4");
                        process::exit(1);
                    });
            }
            "--no-auto-zeta" => {
                auto_zeta = false;
            }
            "--help" | "-h" => {
                eprintln!("Usage: spinsat [OPTIONS] <instance.cnf>");
                eprintln!();
                eprintln!("Options:");
                eprintln!("  -t, --timeout <secs>   Timeout in seconds (default: 5000)");
                eprintln!("  -s, --seed <n>         Initial random seed (default: 42)");
                eprintln!("  -m, --method <name>    Integration method: euler, trapezoid, rk4 (default: euler)");
                eprintln!("  -z, --zeta <val>       Learning rate (default: auto by ratio)");
                eprintln!("      --no-auto-zeta     Disable auto zeta selection");
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
    let formula = match parser::parse_dimacs(&cnf_path) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("c Error parsing {}: {}", cnf_path, e);
            println!("s UNKNOWN");
            process::exit(0);
        }
    };

    let ratio = formula.num_clauses() as f64 / formula.num_vars as f64;

    // Set parameters
    let mut params = Params::default();
    if auto_zeta {
        params = params.with_auto_zeta(ratio);
    }
    if let Some(z) = zeta {
        params.zeta = z;
    }

    eprintln!("c SpinSAT v0.3.0 — DMM-based SAT solver");
    eprintln!(
        "c Instance: {} variables, {} clauses (ratio {:.2})",
        formula.num_vars,
        formula.num_clauses(),
        ratio,
    );
    eprintln!(
        "c Parameters: method={:?}, zeta={:.0e}, seed={}",
        method, params.zeta, seed
    );

    // Configure solver
    let config = SolverConfig {
        timeout_secs: timeout,
        initial_seed: seed,
        method,
        ..Default::default()
    };

    // Solve
    match solve(&formula, &params, &config) {
        SolveResult::Sat(assignment) => {
            println!("s SATISFIABLE");
            print_assignment(&assignment);
        }
        SolveResult::Unknown => {
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
