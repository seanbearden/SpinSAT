use std::fs;
use std::path::Path;

use crate::formula::Formula;

/// Parse a DIMACS CNF file into a Formula.
/// Supports arbitrary clause widths (k-SAT).
pub fn parse_dimacs<P: AsRef<Path>>(path: P) -> Result<Formula, String> {
    let content = fs::read_to_string(path).map_err(|e| format!("Failed to read file: {}", e))?;

    let mut num_vars = 0usize;
    let mut num_clauses = 0usize;
    let mut clauses: Vec<Vec<i32>> = Vec::new();
    let mut current_clause: Vec<i32> = Vec::new();
    let mut header_found = false;

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('c') || line.starts_with('%') {
            continue;
        }
        if line.starts_with("p cnf") || line.starts_with("p CNF") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 4 {
                return Err("Invalid header line".into());
            }
            num_vars = parts[2]
                .parse()
                .map_err(|_| "Invalid variable count".to_string())?;
            num_clauses = parts[3]
                .parse()
                .map_err(|_| "Invalid clause count".to_string())?;
            header_found = true;
            continue;
        }
        if !header_found {
            continue;
        }

        // Parse clause literals
        for token in line.split_whitespace() {
            let lit: i32 = match token.parse() {
                Ok(v) => v,
                Err(_) => continue,
            };
            if lit == 0 {
                if !current_clause.is_empty() {
                    clauses.push(std::mem::take(&mut current_clause));
                }
            } else {
                current_clause.push(lit);
            }
        }
    }
    // Handle clause not terminated by 0
    if !current_clause.is_empty() {
        clauses.push(current_clause);
    }

    if !header_found {
        return Err("No 'p cnf' header found".into());
    }

    if clauses.len() != num_clauses {
        eprintln!(
            "c Warning: header says {} clauses, found {}",
            num_clauses,
            clauses.len()
        );
    }

    Ok(Formula::new(num_vars, clauses))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn write_temp_cnf(content: &str) -> NamedTempFile {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        f
    }

    #[test]
    fn test_parse_simple_3sat() {
        let cnf = "c test\np cnf 5 3\n1 -5 4 0\n-1 5 3 0\n-3 -4 0\n";
        let f = write_temp_cnf(cnf);
        let formula = parse_dimacs(f.path()).unwrap();
        assert_eq!(formula.num_vars, 5);
        assert_eq!(formula.num_clauses(), 3);
    }

    #[test]
    fn test_parse_multiline_clause() {
        let cnf = "p cnf 3 1\n1 -2\n3 0\n";
        let f = write_temp_cnf(cnf);
        let formula = parse_dimacs(f.path()).unwrap();
        assert_eq!(formula.num_clauses(), 1);
        assert_eq!(formula.clause_width(0), 3);
    }
}
