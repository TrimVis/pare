import os
import sys
import argparse
import subprocess
import time
import json
import shutil
import signal
from pathlib import Path

from .benchmark import run_benchmark
from .gcov import gcov_init, gcov_cleanup, gen_json_reports

def handle_interrupt(signum, frame):
    print("Interrupt received, stopping the script.")
    sys.exit(1)


def main():
    signal.signal(signal.SIGINT, handle_interrupt)

    parser = argparse.ArgumentParser(description='Benchmark coverage script.')
    parser.add_argument('-b', '--build-dir', required=True, help='Build directory')
    parser.add_argument('-a', '--cvc5-args', default='', help='Arguments for cvc5')
    parser.add_argument('-n', '--sample-size', default='all', help='Sample size ("all", or comma-separated values)')
    parser.add_argument('-i', '--individual', action='store_true', help='Use individual GCOV prefixes for each run')
    parser.add_argument('-r', '--no-runs', default=1, type=int, help='Number of runs')
    parser.add_argument('-s', '--run-start-no', default=1, type=int, help='Start index for runs')
    parser.add_argument('-j', '--job-size', default=1, type=int, help='Number of parallel jobs')
    parser.add_argument('-f', '--full-report', action='store_true',help='Generate lcov as well as fastcov report')
    parser.add_argument('-v', '--verbose', action='store_true', help='Verbose output')
    parser.add_argument('benchmark_dir', help='Benchmark directory')
    parser.add_argument('output_dir', help='Output directory')
    args = parser.parse_args()

    out = sys.stdout if args.verbose else open(os.devnull, 'w')
    build_dir = Path(args.build_dir).resolve()
    bench_dir = Path(args.benchmark_dir).resolve()
    out_dir = Path(args.output_dir).resolve()
    out_dir.mkdir(parents=True, exist_ok=True)

    # cwd = os.getcwd()

    # Change to build directory
    # os.chdir(build_dir)

    sample_sizes = args.sample_size.split(',')
    cvc5_executable = build_dir / 'bin' / 'cvc5'
    if not cvc5_executable.is_file():
        print(f"Error: cvc5 executable not found at {cvc5_executable}")
        sys.exit(1)

    # Reset coverage & create necessary folders
    gcov_init(bench_dir)

    for sample_size in sample_sizes:
        for run_number in range(args.run_start_no, args.no_runs + 1):
            cmd = [str(cvc5_executable)] + args.cvc5_args.split()
            bname = out_dir / f's{sample_size}_{run_number}'

            print(f"[{time.strftime('%Y-%m-%d %H:%M:%S')}] Sample Size: {sample_size} \tArgs: {args.cvc5_args} \trun: {run_number}/{args.no_runs}")

            # The run_benchmark function handles sampling, execution and logging
            run_benchmark(sample_size, str(bench_dir), args.job_size, cmd, bname, use_prefix=args.individual)

    # Now handled through run_benchmark
    # gen_json_reports(job_size=args.job_size, verbose=args.verbose)

    # Reset coverage & remove folders
    gcov_cleanup()


    print("exit")

if __name__ == '__main__':
    main()
