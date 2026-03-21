/*
 * Fast SAT solution checker.
 * Usage: check_sat <instance.cnf> <solver_output.txt>
 *
 * Reads a DIMACS CNF file and a solver output file (with "s SATISFIABLE" and "v ... 0" lines),
 * verifies every clause is satisfied by the assignment.
 */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define MAX_VARS 2000000
#define MAX_LINE 65536

static int assignment[MAX_VARS + 1]; /* 1=true, -1=false, 0=unset */

int main(int argc, char **argv) {
    if (argc < 3) {
        fprintf(stderr, "Usage: %s <instance.cnf> <solver_output.txt>\n", argv[0]);
        return 1;
    }

    /* Parse solver output for assignment */
    FILE *sol = fopen(argv[2], "r");
    if (!sol) { fprintf(stderr, "Cannot open %s\n", argv[2]); return 1; }

    char line[MAX_LINE];
    int found_sat = 0;
    memset(assignment, 0, sizeof(assignment));

    while (fgets(line, sizeof(line), sol)) {
        if (line[0] == 's' && line[1] == ' ') {
            if (strstr(line, "SATISFIABLE")) found_sat = 1;
        }
        if (line[0] == 'v' && line[1] == ' ') {
            char *p = line + 2;
            int lit;
            while (sscanf(p, "%d", &lit) == 1) {
                if (lit == 0) break;
                int var = abs(lit);
                if (var <= MAX_VARS)
                    assignment[var] = (lit > 0) ? 1 : -1;
                while (*p == ' ' || *p == '\t') p++;
                if (*p == '-') p++;
                while (*p >= '0' && *p <= '9') p++;
            }
        }
    }
    fclose(sol);

    if (!found_sat) {
        printf("SKIP: solver did not report SATISFIABLE\n");
        return 0;
    }

    /* Verify against CNF */
    FILE *cnf = fopen(argv[1], "r");
    if (!cnf) { fprintf(stderr, "Cannot open %s\n", argv[1]); return 1; }

    int num_vars = 0, num_clauses = 0;
    int clause_sat = 0;
    int clause_has_lits = 0;
    int failed = 0;
    int total_clauses = 0;

    while (fgets(line, sizeof(line), cnf)) {
        if (line[0] == 'c' || line[0] == '%' || line[0] == '\n' || line[0] == '\r')
            continue;
        if (line[0] == 'p') {
            sscanf(line, "p cnf %d %d", &num_vars, &num_clauses);
            continue;
        }

        char *p = line;
        int lit;
        while (sscanf(p, "%d", &lit) == 1) {
            if (lit == 0) {
                if (clause_has_lits) {
                    total_clauses++;
                    if (!clause_sat) {
                        fprintf(stderr, "FAIL: clause %d not satisfied\n", total_clauses);
                        failed++;
                    }
                }
                clause_sat = 0;
                clause_has_lits = 0;
            } else {
                clause_has_lits = 1;
                int var = abs(lit);
                if ((lit > 0 && assignment[var] == 1) ||
                    (lit < 0 && assignment[var] == -1)) {
                    clause_sat = 1;
                }
            }
            while (*p == ' ' || *p == '\t') p++;
            if (*p == '-') p++;
            while (*p >= '0' && *p <= '9') p++;
        }
    }
    fclose(cnf);

    if (failed == 0) {
        printf("OK: all %d clauses satisfied (%d variables)\n", total_clauses, num_vars);
        return 0;
    } else {
        printf("FAIL: %d/%d clauses not satisfied\n", failed, total_clauses);
        return 1;
    }
}
