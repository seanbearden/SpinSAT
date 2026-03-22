#!/usr/bin/env python3
"""
Analyze SpinSAT solution path trace files.

Reads binary trace files produced by `spinsat --trace full|snapshot` and
generates visualizations of the discrete Boolean trajectory.

Usage:
    python3 scripts/analyze_trace.py trace.bin [--output-dir plots/]
    python3 scripts/analyze_trace.py trace.bin --summary   # text summary only
"""

import argparse
import struct
import sys
from pathlib import Path

import numpy as np

# Header format: SPTR(4) + version(1) + mode(1) + num_vars(4) + num_clauses(4) + flags(1) = 15
HEADER_SIZE = 15
MAGIC = b"SPTR"
FLIP_RECORD_SIZE = 13  # f64 + u32 + u8
RESTART_MARKER_VAR = 0xFFFFFFFF
RESTART_MARKER_VAL = 0xFF


def read_header(f):
    """Read and validate trace file header."""
    data = f.read(HEADER_SIZE)
    if len(data) < HEADER_SIZE:
        raise ValueError("File too short for header")

    magic = data[0:4]
    if magic != MAGIC:
        raise ValueError(f"Bad magic: {magic!r}, expected {MAGIC!r}")

    version = data[4]
    mode = data[5]  # 0=Full, 1=Snapshot
    num_vars = struct.unpack("<I", data[6:10])[0]
    num_clauses = struct.unpack("<I", data[10:14])[0]
    flags = data[14]
    trace_memory = bool(flags & 1)

    interval = None
    if mode == 1:  # Snapshot
        interval_data = f.read(8)
        interval = struct.unpack("<Q", interval_data)[0]

    return {
        "version": version,
        "mode": "full" if mode == 0 else "snapshot",
        "num_vars": num_vars,
        "num_clauses": num_clauses,
        "trace_memory": trace_memory,
        "snapshot_interval": interval,
    }


def read_full_trace(f, header):
    """Read flip events from a full-mode trace file."""
    events = []
    restarts = []
    data = f.read()

    offset = 0
    while offset + FLIP_RECORD_SIZE <= len(data):
        t = struct.unpack_from("<d", data, offset)[0]
        var = struct.unpack_from("<I", data, offset + 8)[0]
        val = data[offset + 12]
        offset += FLIP_RECORD_SIZE

        if var == RESTART_MARKER_VAR and val == RESTART_MARKER_VAL:
            restarts.append(t if not np.isnan(t) else (events[-1][0] if events else 0.0))
        else:
            events.append((t, var, val))

    if not events:
        return None, restarts

    arr = np.array(events, dtype=[("time", "f8"), ("var", "u4"), ("val", "u1")])
    return arr, restarts


def read_snapshot_trace(f, header):
    """Read snapshots from a snapshot-mode trace file."""
    num_vars = header["num_vars"]
    packed_bytes = (num_vars + 7) // 8
    record_size = 8 + packed_bytes  # f64 time + packed bools

    data = f.read()
    snapshots = []

    offset = 0
    while offset + record_size <= len(data):
        t = struct.unpack_from("<d", data, offset)[0]
        packed = data[offset + 8 : offset + 8 + packed_bytes]
        offset += record_size

        assignment = np.zeros(num_vars, dtype=np.uint8)
        for i in range(num_vars):
            if packed[i // 8] & (1 << (i % 8)):
                assignment[i] = 1

        snapshots.append((t, assignment))

    return snapshots


def print_summary(header, events, restarts):
    """Print text summary of the trace."""
    print(f"Trace file summary:")
    print(f"  Mode: {header['mode']}")
    print(f"  Variables: {header['num_vars']}")
    print(f"  Clauses: {header['num_clauses']}")
    print(f"  Memory traced: {header['trace_memory']}")

    if header["mode"] == "full" and events is not None:
        print(f"  Total flip events: {len(events):,}")
        print(f"  Restarts: {len(restarts)}")
        print(f"  Time range: [{events['time'].min():.2f}, {events['time'].max():.2f}]")

        # Per-variable flip counts
        var_counts = np.bincount(events["var"], minlength=header["num_vars"])
        print(f"  Flips per variable:")
        print(f"    Mean: {var_counts.mean():.1f}")
        print(f"    Max:  {var_counts.max()} (var {var_counts.argmax()})")
        print(f"    Min:  {var_counts.min()} (var {var_counts.argmin()})")

        # Most active variables
        top_10 = np.argsort(var_counts)[-10:][::-1]
        print(f"  Top 10 most active variables:")
        for v in top_10:
            print(f"    var {v}: {var_counts[v]:,} flips")

        # Least active
        bottom_5 = np.argsort(var_counts)[:5]
        print(f"  5 least active variables:")
        for v in bottom_5:
            print(f"    var {v}: {var_counts[v]:,} flips")

    elif header["mode"] == "snapshot":
        print(f"  Snapshot interval: {header['snapshot_interval']} steps")


def plot_full_trace(header, events, restarts, output_dir):
    """Generate visualizations for full-mode trace."""
    import matplotlib.pyplot as plt

    output_dir = Path(output_dir)
    output_dir.mkdir(parents=True, exist_ok=True)

    num_vars = header["num_vars"]
    times = events["time"]
    vars_ = events["var"]

    # 1. Flip rate over time
    fig, ax = plt.subplots(figsize=(12, 4))
    n_bins = min(500, max(50, len(events) // 1000))
    ax.hist(times, bins=n_bins, color="steelblue", edgecolor="none", alpha=0.8)
    for rt in restarts:
        ax.axvline(rt, color="red", linewidth=0.5, alpha=0.7)
    ax.set_xlabel("Integration time")
    ax.set_ylabel("Flips per bin")
    ax.set_title("Flip rate over time (red = restarts)")
    plt.tight_layout()
    plt.savefig(output_dir / "flip_rate.png", dpi=150)
    plt.close()

    # 2. Variable activity histogram
    fig, ax = plt.subplots(figsize=(12, 4))
    var_counts = np.bincount(vars_, minlength=num_vars)
    sorted_counts = np.sort(var_counts)[::-1]
    ax.bar(range(num_vars), sorted_counts, width=1.0, color="steelblue", edgecolor="none")
    ax.set_xlabel("Variable rank (most to least active)")
    ax.set_ylabel("Total flips")
    ax.set_title("Variable activity distribution")
    plt.tight_layout()
    plt.savefig(output_dir / "variable_activity.png", dpi=150)
    plt.close()

    # 3. Flip heatmap (time bins x variable)
    fig, ax = plt.subplots(figsize=(14, 8))
    time_bins = min(200, max(20, len(events) // 5000))
    t_edges = np.linspace(times.min(), times.max(), time_bins + 1)
    heatmap = np.zeros((num_vars, time_bins))
    t_bin_idx = np.clip(np.digitize(times, t_edges) - 1, 0, time_bins - 1)
    for i in range(len(events)):
        heatmap[vars_[i], t_bin_idx[i]] += 1

    # Sort variables by total activity for better visualization
    activity_order = np.argsort(var_counts)[::-1]
    heatmap_sorted = heatmap[activity_order, :]

    im = ax.imshow(
        heatmap_sorted,
        aspect="auto",
        cmap="hot",
        interpolation="nearest",
        extent=[times.min(), times.max(), num_vars, 0],
    )
    for rt in restarts:
        ax.axvline(rt, color="cyan", linewidth=0.5, alpha=0.5)
    plt.colorbar(im, label="Flips per bin")
    ax.set_xlabel("Integration time")
    ax.set_ylabel("Variable (sorted by activity)")
    ax.set_title("Flip heatmap")
    plt.tight_layout()
    plt.savefig(output_dir / "flip_heatmap.png", dpi=150)
    plt.close()

    # 4. Oscillation frequency over time (top 10 most active vars)
    fig, ax = plt.subplots(figsize=(12, 6))
    top_vars = np.argsort(var_counts)[-10:][::-1]
    window_bins = 50
    t_window_edges = np.linspace(times.min(), times.max(), window_bins + 1)

    for v in top_vars:
        mask = vars_ == v
        v_times = times[mask]
        freq, _ = np.histogram(v_times, bins=t_window_edges)
        bin_centers = (t_window_edges[:-1] + t_window_edges[1:]) / 2
        ax.plot(bin_centers, freq, label=f"var {v}", alpha=0.7)

    ax.set_xlabel("Integration time")
    ax.set_ylabel("Flips per window")
    ax.set_title("Oscillation frequency — top 10 variables")
    ax.legend(fontsize=8, ncol=2)
    plt.tight_layout()
    plt.savefig(output_dir / "oscillation_frequency.png", dpi=150)
    plt.close()

    # 5. Cumulative flips (convergence indicator)
    fig, ax = plt.subplots(figsize=(12, 4))
    cumulative = np.arange(1, len(events) + 1)
    ax.plot(times, cumulative, color="steelblue", linewidth=0.5)
    for rt in restarts:
        ax.axvline(rt, color="red", linewidth=0.5, alpha=0.7)
    ax.set_xlabel("Integration time")
    ax.set_ylabel("Cumulative flips")
    ax.set_title("Cumulative flip count")
    plt.tight_layout()
    plt.savefig(output_dir / "cumulative_flips.png", dpi=150)
    plt.close()

    print(f"Plots saved to {output_dir}/")


def plot_snapshot_trace(header, snapshots, output_dir):
    """Generate visualizations for snapshot-mode trace."""
    import matplotlib.pyplot as plt

    output_dir = Path(output_dir)
    output_dir.mkdir(parents=True, exist_ok=True)

    num_vars = header["num_vars"]
    times = [s[0] for s in snapshots]
    assignments = np.array([s[1] for s in snapshots])  # shape: (n_snapshots, num_vars)

    # 1. Assignment evolution heatmap
    fig, ax = plt.subplots(figsize=(14, 8))
    ax.imshow(
        assignments.T,
        aspect="auto",
        cmap="binary",
        interpolation="nearest",
        extent=[times[0], times[-1], num_vars, 0],
    )
    ax.set_xlabel("Integration time")
    ax.set_ylabel("Variable index")
    ax.set_title("Assignment evolution")
    plt.tight_layout()
    plt.savefig(output_dir / "assignment_evolution.png", dpi=150)
    plt.close()

    # 2. Hamming distance between consecutive snapshots
    if len(assignments) > 1:
        fig, ax = plt.subplots(figsize=(12, 4))
        hamming = np.sum(assignments[1:] != assignments[:-1], axis=1)
        ax.plot(times[1:], hamming, color="steelblue", linewidth=0.5)
        ax.set_xlabel("Integration time")
        ax.set_ylabel("Hamming distance")
        ax.set_title("Hamming distance between consecutive snapshots")
        plt.tight_layout()
        plt.savefig(output_dir / "hamming_distance.png", dpi=150)
        plt.close()

    print(f"Plots saved to {output_dir}/")


def main():
    parser = argparse.ArgumentParser(description="Analyze SpinSAT trace files")
    parser.add_argument("trace_file", help="Path to trace.bin file")
    parser.add_argument("--output-dir", "-o", default="trace_plots",
                        help="Output directory for plots (default: trace_plots)")
    parser.add_argument("--summary", action="store_true",
                        help="Print text summary only, no plots")
    args = parser.parse_args()

    with open(args.trace_file, "rb") as f:
        header = read_header(f)

        if header["mode"] == "full":
            events, restarts = read_full_trace(f, header)
            if events is None:
                print("No flip events found in trace.")
                return
            print_summary(header, events, restarts)
            if not args.summary:
                plot_full_trace(header, events, restarts, args.output_dir)

        elif header["mode"] == "snapshot":
            snapshots = read_snapshot_trace(f, header)
            print_summary(header, None, [])
            print(f"  Snapshots: {len(snapshots)}")
            if not args.summary and snapshots:
                plot_snapshot_trace(header, snapshots, args.output_dir)


if __name__ == "__main__":
    main()
