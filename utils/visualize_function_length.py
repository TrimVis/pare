#!/usr/bin/env python3
import argparse
import sqlite3
import matplotlib.pyplot as plt
import numpy as np


def query_data(db_file):
    conn = sqlite3.connect(db_file)
    cur = conn.cursor()
    query = """
    SELECT count(*), f.benchmark_usage_count
    FROM "functions" AS f
    GROUP BY f.benchmark_usage_count
    ORDER BY f.benchmark_usage_count;
    """
    cur.execute(query)
    data = cur.fetchall()
    conn.close()
    return data


def main():
    parser = argparse.ArgumentParser(
        description="Create a 1D histogram (bar chart) of benchmark usage count from SQLite query data."
    )
    parser.add_argument("db", help="Path to the SQLite database file")
    parser.add_argument("--output", "-o", help="Optional output SVG file path")
    parser.add_argument("--logscale", action="store_true",
                        help="Group data into logarithmically spaced buckets")
    parser.add_argument("--bins", type=int, default=10,
                        help="Number of bins when logscale is enabled (default: 10)")
    args = parser.parse_args()

    data = query_data(args.db)
    if not data:
        print("No data returned from query.")
        return

    # Each row is (count, benchmark_usage_count)
    counts, usage = zip(*data)
    counts = np.array(counts)
    usage = np.array(usage)

    if args.logscale:
        # Only consider positive usage values for log-scale grouping.
        mask = usage > 0
        if not np.any(mask):
            print("No positive usage values available for logscale grouping.")
            return
        usage_positive = usage[mask]
        counts_positive = counts[mask]

        # Create logarithmically spaced bin edges.
        bin_edges = np.logspace(np.log10(usage_positive.min()), np.log10(
            usage_positive.max()), args.bins + 1)
        # Determine which bin each value belongs to.
        bin_indices = np.digitize(usage_positive, bin_edges, right=False) - 1
        # Aggregate counts per bin.
        agg_counts = np.zeros(args.bins)
        for idx, cnt in zip(bin_indices, counts_positive):
            if idx < 0:
                idx = 0
            elif idx >= args.bins:
                idx = args.bins - 1
            agg_counts[idx] += cnt
        # Compute bin centers for plotting.
        bin_centers = (bin_edges[:-1] + bin_edges[1:]) / 2
        x_values = bin_centers
        y_values = agg_counts
    else:
        # Group by unique benchmark_usage_count values.
        unique_usage = np.unique(usage)
        agg_counts = np.zeros_like(unique_usage, dtype=float)
        for u, c in zip(usage, counts):
            idx = np.where(unique_usage == u)[0][0]
            agg_counts[idx] += c
        x_values = unique_usage
        y_values = agg_counts

    # Plotting the bar chart.
    plt.figure(figsize=(8, 6))
    # Choose a default bar width; for logscale grouping, width based on bin spacing.
    if args.logscale:
        width = (x_values[1] - x_values[0]) if len(x_values) > 1 else 1.0
    else:
        # For discrete unique values, a constant width
        width = 0.8 * (x_values[1] - x_values[0]) if len(x_values) > 1 else 1.0

    plt.bar(x_values, y_values, width=width,
            color='skyblue', edgecolor='black')
    plt.xlabel("Benchmark Usage Count")
    plt.ylabel("Count")
    plt.title("Histogram: Benchmark Usage Count")
    plt.grid(axis='y', linestyle='--', linewidth=0.5)

    # (Do not set the axis scales to log; we only group data by log-scale.)

    if args.output:
        plt.savefig(args.output, format='svg', bbox_inches='tight')
        print(f"Figure saved to {args.output}")
    else:
        plt.show()


if __name__ == "__main__":
    main()
