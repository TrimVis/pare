#!/bin/bash

# Function to print usage
usage() {
    echo "Usage: $0 --sample-size|-s <sample-size> --benchmark-dir|-d <directory>"
    exit 1
}

# Parse the named arguments
while [[ "$#" -gt 0 ]]; do
    case $1 in
        --sample-size|-s)
            SAMPLE_SIZE="$2"
            shift
            ;;
        --benchmark-dir|-d)
            BENCHMARK_DIR="$2"
            shift
            ;;
        *)
            echo "Unknown parameter: $1"
            usage
            ;;
    esac
    shift
done

# Check if both arguments are provided
if [[ -z "$SAMPLE_SIZE" || -z "$BENCHMARK_DIR" ]]; then
    usage
fi

# Verify if the benchmark directory exists
if [[ ! -d "$BENCHMARK_DIR" ]]; then
    echo "Error: Directory $BENCHMARK_DIR does not exist."
    exit 1
fi

# Count total files
TOTAL_FILES=$(find "$BENCHMARK_DIR" -type f | wc -l)

if [[ "$SAMPLE_SIZE" -gt "$TOTAL_FILES" ]]; then
    echo "Error: Requested sample size ($SAMPLE_SIZE) is greater than the total number of files ($TOTAL_FILES) in the directory."
    exit 1
fi

# TODO pjordan: FIlter on *.smt
# Use find and shuf to get random files without exceeding the argument list limit
if [ "$SAMPLE_SIZE" = "all" ]; then
    find "$BENCHMARK_DIR" -type f
else
    find "$BENCHMARK_DIR" -type f | shuf -n "$SAMPLE_SIZE"
fi

