#!/bin/bash
# SpinSAT run script for SAT Competition 2026
# $1 = path to DIMACS CNF benchmark instance
# $2 = path to proof output directory (unused — incomplete solver)

exec bin/spinsat-linux-x86_64 --timeout 5000 "$1"
