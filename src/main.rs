mod dmm;
mod formula;
mod integrator;
mod parser;
mod solver;

use std::env;
use std::process;

use dmm::Params;
use solver::{solve, SolveResult};

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: spinsat <instance.cnf>");
        process::exit(1);
    }

    let cnf_path = &args[1];

    // Parse DIMACS CNF
    let formula = match parser::parse_dimacs(cnf_path) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("c Error parsing {}: {}", cnf_path, e);
            println!("s UNKNOWN");
            process::exit(0);
        }
    };

    eprintln!("c SpinSAT v0.1.0 — DMM-based SAT solver");
    eprintln!(
        "c Instance: {} variables, {} clauses",
        formula.num_vars,
        formula.num_clauses()
    );

    // Set parameters
    let params = Params::default();

    // Competition timeout
    let timeout = 5000.0;

    // Solve
    let seed = 42;
    match solve(&formula, &params, timeout, seed) {
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
/// v 1 -2 3 -4 5 0
fn print_assignment(assignment: &[bool]) {
    let mut line = String::from("v ");
    for (i, &val) in assignment.iter().enumerate() {
        let var_num = (i + 1) as i32;
        let lit = if val { var_num } else { -var_num };

        let token = format!("{}", lit);
        // SAT competition: max 4096 chars per value line
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
