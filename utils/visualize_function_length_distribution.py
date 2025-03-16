#!/usr/bin/env python3
import argparse
import sqlite3
import matplotlib.pyplot as plt
import numpy as np
from matplotlib.ticker import MaxNLocator
import numpy.ma as ma

from matplotlib.colors import Normalize


def query_data(db_file, min_benchusage=None, max_benchusage=None):
    conn = sqlite3.connect(db_file)
    cur = conn.cursor()
    filter = f"f.benchmark_usage_count > {
        min_benchusage}" if min_benchusage else ""
    filter += " AND " if filter else ""
    filter += f"f.benchmark_usage_count <= {
        max_benchusage}" if max_benchusage else ""

    query = f"""
    SELECT count(*), f.end_line - f.start_line, f.benchmark_usage_count
    FROM "functions" AS f
    {'WHERE ' + filter if filter else ''}
    GROUP BY f.end_line - f.start_line, f.benchmark_usage_count
    ORDER BY f.end_line - f.start_line, f.benchmark_usage_count
    """
    cur.execute(query)
    data = cur.fetchall()
    conn.close()
    return data


def create_histogram(data, xbuckets=None, ybuckets=None, logscale=False, filter_zero=False):
    # Unpack query results: (count, function size, benchmark usage)
    counts, sizes, usage = zip(*data)
    counts = np.array(counts)
    sizes = np.array(sizes)
    usage = np.array(usage)

    if logscale:
        # Ensure that sizes and usage are positive for logscale grouping.
        # (Non-positive values will be skipped from binning.)
        sizes = sizes[sizes > 0]
        usage = usage[usage > 0]
        if xbuckets is None:
            unique_sizes = np.unique(sizes)
            x_bin_edges = np.concatenate(
                ([unique_sizes[0] / 2], unique_sizes * 1.5))
            x_centers = unique_sizes
            nb_x = len(unique_sizes)
        else:
            x_min, x_max = sizes.min(), sizes.max()
            x_bin_edges = np.logspace(
                np.log10(x_min), np.log10(x_max), xbuckets + 1)
            x_centers = (x_bin_edges[:-1] + x_bin_edges[1:]) / 2
            nb_x = xbuckets

        if ybuckets is None:
            unique_usage = np.unique(usage)
            y_bin_edges = np.concatenate(
                ([unique_usage[0] / 2], unique_usage * 1.5))
            y_centers = unique_usage
            nb_y = len(unique_usage)
        else:
            y_min, y_max = usage.min(), usage.max()
            y_bin_edges = np.logspace(
                np.log10(y_min), np.log10(y_max), ybuckets + 1)
            y_centers = (y_bin_edges[:-1] + y_bin_edges[1:]) / 2
            nb_y = ybuckets
    else:
        if xbuckets is None:
            unique_sizes = np.unique(sizes)
            x_bin_edges = np.concatenate(
                ([unique_sizes[0] - 0.5], unique_sizes + 0.5))
            x_centers = unique_sizes
            nb_x = len(unique_sizes)
        else:
            x_min, x_max = sizes.min(), sizes.max()
            x_bin_edges = np.linspace(x_min, x_max, xbuckets + 1)
            x_centers = (x_bin_edges[:-1] + x_bin_edges[1:]) / 2
            nb_x = xbuckets

        if ybuckets is None:
            unique_usage = np.unique(usage)
            y_bin_edges = np.concatenate(
                ([unique_usage[0] - 0.5], unique_usage + 0.5))
            y_centers = unique_usage
            nb_y = len(unique_usage)
        else:
            y_min, y_max = usage.min(), usage.max()
            y_bin_edges = np.linspace(y_min, y_max, ybuckets + 1)
            y_centers = (y_bin_edges[:-1] + y_bin_edges[1:]) / 2
            nb_y = ybuckets

    # Create a 2D histogram array.
    hist = np.zeros((nb_y + 1, nb_x + 1))
    # Iterate over the data (using the original data for counts).
    for cnt, s, u in data:
        # Skip non-positive values in logscale mode.
        if logscale and (s < 0 or u < 0):
            continue
        if filter_zero and u <= 0:
            continue
        # Determine bucket indices.
        x_idx = np.digitize(s, x_bin_edges, right=False) if s > 0 else 0
        y_idx = np.digitize(u, y_bin_edges, right=False) if u > 0 else 0
        if x_idx >= nb_x:
            x_idx = nb_x
        if y_idx >= nb_y:
            y_idx = nb_y
        hist[y_idx, x_idx] += cnt

    return x_centers, y_centers, hist, x_bin_edges, y_bin_edges


def main():
    parser = argparse.ArgumentParser(
        description="Create a 2D histogram heatmap from SQLite query data."
    )
    parser.add_argument("db", help="Path to the SQLite database file")
    parser.add_argument("--output", "-o", help="Optional output SVG file path")
    parser.add_argument("--xbuckets", type=int, default=0,
                        help="Number of buckets for function size (x-axis); 0 uses unique values")
    parser.add_argument("--ybuckets", type=int, default=0,
                        help="Number of buckets for benchmark usage (y-axis); 0 uses unique values")
    parser.add_argument("--ymin", type=int, default=0)
    parser.add_argument("--ymax", type=int, default=0)
    parser.add_argument("--yticks", type=int, default=0)
    parser.add_argument("--xticks", type=int, default=0)
    parser.add_argument("--filter-zero", action="store_true",
                        help="Filter out 0 values")
    parser.add_argument("--logscale", action="store_true",
                        help="Group the buckets on a logarithmic scale")
    args = parser.parse_args()

    # If bucket parameters are 0, treat them as None.
    xbuckets = args.xbuckets if args.xbuckets > 0 else None
    ybuckets = args.ybuckets if args.ybuckets > 0 else None
    ymin = args.ymin if args.ymin > 0 else None
    ymax = args.ymax if args.ymax > 0 else None

    # Query the database.
    data = query_data(args.db, ymin, ymax)
    if not data:
        print("No data returned from query.")
        return

    # Create histogram data.
    x_centers, y_centers, hist, x_edges, y_edges = create_histogram(
        data, xbuckets, ybuckets, args.logscale, args.filter_zero)

    if not args.filter_zero:
        hist = hist[1:, 1:]

    hist_masked = ma.masked_equal(hist, 0)

    # Get a copy of the viridis colormap and set its "bad" color to white.
    cmap = plt.cm.viridis.copy()
    cmap.set_bad(color='white')

    # Define a normalization (adjust vmin/vmax as needed).
    norm = Normalize(vmin=hist_masked.min(), vmax=hist_masked.max())

    fig, ax = plt.subplots(figsize=(8, 6))
    im = ax.imshow(hist_masked, interpolation='nearest', aspect='auto', origin='lower',
                   extent=[x_edges[0], x_edges[-1], y_edges[0], y_edges[-1]],
                   cmap=cmap, norm=norm)

    cbar = fig.colorbar(im, ax=ax)
    cbar.set_label("Combination Count")

    ax.set_xlabel("Function size")
    ax.set_ylabel("Benchmark usage count")
    # ax.set_title("2D Histogram: Function Size vs. Benchmark Usage Count")

    # Set tick marks at the bin centers.
    if args.xticks > 0:
        ax.xaxis.set_major_locator(MaxNLocator(nbins=args.xticks))
    else:
        ax.set_xticks(x_centers)
    if args.yticks > 0:
        ax.yaxis.set_major_locator(MaxNLocator(nbins=args.yticks))
    else:
        ax.set_yticks(y_centers)
    ax.grid(True, linestyle='--', linewidth=0.5, color='gray', alpha=0.7)

    # Save or display the figure.
    if args.output:
        plt.savefig(args.output, format='svg', bbox_inches='tight')
        print(f"Figure saved to {args.output}")
    else:
        plt.show()


if __name__ == "__main__":
    main()
