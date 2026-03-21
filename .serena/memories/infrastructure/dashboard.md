# GitHub Pages Dashboard (2026-03-21)

## Location
- Source: `docs/dashboard/index.html`
- URL: https://seanbearden.github.io/SpinSAT/dashboard/
- Status: Code deployed, needs GitHub Pages enabled in repo settings

## Setup Required
1. GitHub repo Settings → Pages → Source: "Deploy from a branch"
2. Branch: `main`, Folder: `/docs`
3. Save

## Technology
- **sql.js-httpvfs**: loads benchmarks.db from GitHub Releases via HTTP Range requests
- **Chart.js**: PAR-2 charts, solve time distribution, competition comparison
- **Zero infrastructure**: static HTML/JS, no server needed

## Features
- **Overview tab**: PAR-2 by version (bar chart), solve time distribution (doughnut), recent runs table
- **Version Comparison tab**: best/avg/max times, PAR-2 per run
- **Competition tab**: SpinSAT vs competition solvers (horizontal bar chart + table)
- **SQL Explorer tab**: arbitrary SQL queries against the full DB in-browser

## Data Flow
1. Run benchmarks locally with --record → writes to benchmarks.db
2. Upload DB to release: `gh release upload <tag> benchmarks.db`
3. Dashboard auto-loads latest release DB
4. Fallback: Datasette Lite link in README for ad-hoc SQL

## Datasette Lite URL
https://lite.datasette.io/?url=https://github.com/seanbearden/SpinSAT/releases/latest/download/benchmarks.db
