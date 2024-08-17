#!/bin/bash

# Trap Ctrl+C (SIGINT) and call the handle_interrupt function
handle_interrupt() {
    echo "Interrupt received, stopping the script."
    exit 1
}
trap handle_interrupt SIGINT

# Function to print usage
usage() {
    echo "Usage: $0 [OPTIONS] [BENCHMARK_DIR] [OUTPUT_DIR]

    -b, --build-dir <build-dir>         (required)
    -a, --cvc5-args <cvc5-args>         (optional, default: '')
    -n, --sample-size <sample-size>     (optional, default: 'all')
                                        Can be a comma separated list (\"200,1000,all\")
    -r, --no-runs <no-runs>             (optional, default: 1)
    -s, --run-start-no <start-index>    (optional, default: 1)
    -j, --job-size <job-size>           (optional, default: 1)
    -v, --verbose                       (optional, output to stdout)
    -h, --help                          Show this help message and exit

    Example:
    $0 -n 10 -b ../cvc5-repo/build -a \"--tlimit=4000\" /path/to/benchmarks/ /path/to/output/
    "
    exit 1
}

# Default values
OUT="/dev/null"
START_NO=1
NO_RUNS=1
PARALLELIZE=1
ARGS=""
BUILD_DIR=""
SAMPLE_SIZES="all"
BENCH_DIR=""
OUT_DIR=""
SCRIPTS="$(realpath "$(dirname "$0")")"

# Parse the named arguments
while [[ "$#" -gt 2 ]]; do
    case $1 in
    -h | --help)
        usage
        ;;
    -b | --build-dir)
        BUILD_DIR="$(realpath "$2")"
        shift
        ;;
    -n | --sample-size)
        SAMPLE_SIZES="$2"
        shift
        ;;
    -a | --cvc5-args)
        ARGS="$2"
        shift
        ;;
    -s | --run-start-no)
        START_NO="$2"
        shift
        ;;
    -r | --no-runs)
        NO_RUNS="$2"
        shift
        ;;
    -j | --job-size)
        PARALLELIZE="$2"
        shift
        ;;
    -v | --verbose)
        OUT="/dev/stdout"
        ;;
    *)
        echo "Unknown parameter: $1"
        usage
        ;;
    esac
    shift
done

# Capture the last two positional arguments
if [[ "$#" -ne 2 ]]; then
    echo "Error: BENCHMARK_DIR and OUTPUT_DIR are required."
    usage
fi

BENCH_DIR="$(realpath "$1")"
OUT_DIR="$(realpath "$2")"

# Validate required arguments
if [[ -z "$SAMPLE_SIZES" || -z "$BUILD_DIR" ]]; then
    echo "Error: --sample-size and --build-dir are required."
    usage
fi

# Validate BENCH_DIR
if [[ ! -d "$BENCH_DIR" ]]; then
    echo "Error: Benchmark directory '$BENCH_DIR' does not exist."
    exit 1
fi

# Create the result directory if it doesn't exist
if [[ ! -d "$OUT_DIR" ]]; then
    if ! mkdir -p "$OUT_DIR"; then
        echo "Error: Failed to create output directory '$OUT_DIR'."
        exit 1
    fi
fi

# Go into build dir
cd "$BUILD_DIR" || {
    echo "Error: Could not change to build directory '$BUILD_DIR'."
    exit 1
}

IFS=','
for SAMPLE_SIZE in $SAMPLE_SIZES; do
    echo "$SAMPLE_SIZE"
    for ((r = START_NO; r <= NO_RUNS; r++)); do
        echo "$r"
        BNAME="${OUT_DIR}/s${SAMPLE_SIZE}_${r}"
        CMD="${BUILD_DIR}/bin/cvc5 $ARGS"

        echo -e "[$(date -u "+%Y-%m-%d %H:%M:%S")] Sample Size: ${SAMPLE_SIZE} \tArgs: $ARGS \trun: $r/$NO_RUNS"

        if ! make coverage-reset &> "$OUT"; then
            echo "Error: Failed to reset coverage."
            exit 1
        fi

        "${SCRIPTS}/run_benchmark.sh" \
            -n "$SAMPLE_SIZE" -j "$PARALLELIZE" -b "$BENCH_DIR" \
            -c "$CMD" | tee "${BNAME}.log"

        # This depends on "make coverage-json", so the 
        # coverage.json file will also be generated
        if ! make coverage &> "$OUT"; then
            echo "Error: Failed to generate coverage."
            exit 1
        fi

        if ! cp -r coverage "${BNAME}_report" &> "$OUT"; then
            echo "Error: Failed to copy coverage HTML report."
            exit 1
        fi

        if ! cp coverage.json "${BNAME}.json" &> "$OUT"; then
            echo "Error: Failed to copy coverage json report."
            exit 1
        fi
    done
done
unset IFS

echo "exit"
