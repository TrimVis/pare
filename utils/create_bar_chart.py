#!/usr/bin/env python3
import re
import sys
from collections import defaultdict


def process_block(block):
    """Extract (theory, x, y) tuples from a block."""
    # Extract the x value from the solution filename
    x_match = re.search(r"solution_benchopt_p(\d+\.\d+)\.sol", block)
    if not x_match:
        return []
    x_value = x_match.group(1)
    # Optionally, trim x_value to first 4 characters and convert to float.
    # (You may adjust the formatting as needed.)
    x_value = float(x_value[0:4])

    # Extract the overview section
    overview_match = re.search(
        r"Overview of working benchmarks per theory:(.*)", block, re.DOTALL)
    if not overview_match:
        return []
    overview_text = overview_match.group(1)

    # Regex to extract theory name and numeric value (inside parentheses)
    line_regex = re.compile(r"(\w+):\s+\d+% \(([-\d]+)\)")

    tuples = []
    for match in line_regex.finditer(overview_text):
        theory = match.group(1)
        y_value = abs(int(match.group(2)))
        tuples.append((theory, x_value, y_value))
    return tuples


def main():
    # Read all input from stdin
    input_text = sys.stdin.read()

    # Split the input into blocks starting with "Evaluating solution file"
    blocks = re.split(r"(?=Evaluating solution file )", input_text)

    # Global grouping: theory -> list of (x, y) coordinates
    global_groups = defaultdict(list)

    for block in blocks:
        block = block.strip()
        if not block:
            continue
        tuples = process_block(block)
        for theory, x, y in tuples:
            global_groups[theory].append((x, y))

    # For each theory, sort coordinates by x value (or by y, if preferred)
    for theory in global_groups:
        global_groups[theory] = sorted(
            global_groups[theory], key=lambda tup: tup[0])

    # Build and print LaTeX commands, sorted by theory name
    output_lines = []
    legend_lines = []
    group_sum = sum([sum([x[1] for x in theory])
                    for theory in global_groups.values()])
    misc_theory = {}

    for theory in sorted(global_groups.keys()):
        if all([x[1] == 0 for x in global_groups[theory]]):
            continue

        # Only include important groups, and the rest push into misc style
        if sum([x[1] for x in global_groups[theory]]) / group_sum > 0.02:
            iout = []
            iout.append(
                r"\addplot+[style=" + theory + "style] coordinates {")
            for x, y in global_groups[theory]:
                iout.append("(" + str(x) + ", " + str(y) + ")")
            iout.append("};")
            output_lines.append(" ".join(iout))
            legend_lines.append(theory.replace("_", "\\_"))
        else:
            for x, y in global_groups[theory]:
                misc_theory[x] = (misc_theory.get(x) or 0) + y

    # Append misc style
    misc_theory = sorted(
        [(x, y) for (x, y) in misc_theory.items()], key=lambda tup: tup[0])
    iout = []
    iout.append(
        r"\addplot+[style=MISCstyle] coordinates {")
    for x, y in misc_theory:
        iout.append("(" + str(x) + ", " + str(y) + ")")
    iout.append("};")
    output_lines.append(" ".join(iout))
    legend_lines.append("MISC")

    print("\n".join(output_lines))
    print("\n\n\\legend{" + ", ".join(legend_lines) + "}")


if __name__ == "__main__":
    main()
