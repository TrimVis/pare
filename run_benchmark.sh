#!/bin/bash

# Trap Ctrl+C (SIGINT) and call the handle_interrupt function
handle_interrupt() {
    echo "Interrupt received, stopping the script."
    exit 1
}
trap handle_interrupt SIGINT

# Function to print usage
usage() {
    echo "Usage: $0 -j <no-instances> -n <sample-size> -b <directory> -c <command>"
    echo "
    Options:
    -n, --sample-size <sample-size>       (required) Number of samples to run
    -b, --benchmark-dir <directory>       (required) Directory containing the benchmark files
    -j, --job-size <no-instances>         (optional, default: 1) Number of parallel instances to run
    -c, --cmd <command>                   (required) Command to execute for each file
    -h, --help                            Show this help message and exit

    Example:
    $0 -n 10 -b /path/to/benchmarks -j 4 -c 'cvc5 --tlimit=4000'
    "
    exit 1
}

JOB_SIZE="1"
SCRIPTS="$(realpath "$(dirname "$0")")"

# Parse the named arguments
while [[ "$#" -gt 0 ]]; do
    case $1 in
    -n | --sample-size)
        SAMPLE_SIZE="$2"
        shift
        ;;
    -b | --benchmark-dir)
        BENCHMARK_DIR="$2"
        shift
        ;;
    -j | --job-size)
        JOB_SIZE="$2"
        shift
        ;;
    -c | --cmd)
        CMD_ARG="$2"
        shift
        ;;
    -h | --help)
        usage
        ;;
    *)
        echo "Unknown parameter: $1"
        usage
        ;;
    esac
    shift
done

# Check if all required arguments are provided
if [[ -z "$SAMPLE_SIZE" || -z "$BENCHMARK_DIR" || -z "$CMD_ARG" ]]; then
    echo "Error: Missing required arguments."
    usage
fi

# Validate BENCHMARK_DIR
if [[ ! -d "$BENCHMARK_DIR" ]]; then
    echo "Error: Benchmark directory '$BENCHMARK_DIR' does not exist."
    exit 1
fi


# Run the file_samples.sh script to retrieve files
FILES=$("${SCRIPTS}/file_samples.sh" --sample-size "$SAMPLE_SIZE" --benchmark-dir "$BENCHMARK_DIR")
overall_start_time=$(date +%s%3N)

# Function to process each file
process_file() {
    local start_time end_time duration
    local file="$1"

    printf "| File: %s \n" "$file"
    start_time=$(date +%s%3N)
    $CMD_ARG "$file"
    end_time=$(date +%s%3N)
    duration=$((end_time - start_time))
    printf "\-> Execution Time: %s ms\n" "$duration"
}

export -f process_file
export CMD_ARG

# Run commands either in parallel or sequentially
if [[ "$JOB_SIZE" -ne "1" ]]; then
    echo "$FILES" | parallel -j "$JOB_SIZE" process_file {}
else
    for file in $FILES; do
        process_file "$file"
    done
fi

# Record the overall end time
overall_end_time=$(date +%s%3N)
overall_duration=$((overall_end_time - overall_start_time))

echo ""
echo "-------------------------------------"
echo ""
printf "\=> Overall Execution Time: %s ms\n" "$overall_duration"
