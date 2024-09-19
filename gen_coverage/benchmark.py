import os
import sys
import glob
import time
import shutil
import random
import subprocess
import json
from pathlib import Path
from concurrent.futures import ThreadPoolExecutor, as_completed

from .gcov import get_gcov_env

def sample_files(sample_size, benchmark_dir):
    """Sample files from the benchmark directory."""
    # Verify if the benchmark directory exists
    if not os.path.isdir(benchmark_dir):
        print(f"Error: Directory {benchmark_dir} does not exist.")
        sys.exit(1)
    
    # Collect all files with .smt extension
    all_files = glob.glob(os.path.join(benchmark_dir, "**/*.smt2"), recursive=True)
    total_files = len(all_files)
    
    if sample_size == 'all':
        return all_files
    else:
        sample_size_int = int(sample_size)
        if sample_size_int > total_files:
            print(f"Error: Requested sample size ({sample_size_int}) is greater than the total number of files ({total_files}) in the directory.")
            sys.exit(1)
        return random.sample(all_files, sample_size_int)

def process_file(file, cmd_arg, use_prefix=False):
    """Process a single file with the given command."""
    print(f"[{time.strftime('%Y-%m-%d %H:%M:%S')}] File: {file}")
    res = f"| File: {file}\n"
    start_time = time.time()
    try:
        if use_prefix:
            result = subprocess.run(cmd_arg + [file], env=get_gcov_env(file), check=True, capture_output=True, text=True,)
        else:
            result = subprocess.run(cmd_arg + [file], check=True, capture_output=True, text=True,)
        res += result.stdout
        print(result.stdout)
    except subprocess.CalledProcessError as e:
        res += f"Error processing file {file}: {e}\n"
    duration = (time.time() - start_time) * 1000  # Convert to milliseconds
    res += f"-> Execution Time: {duration:.2f} ms\n"

    return res

def run_benchmark(sample_size, benchmark_dir, job_size, cmd_arg, bname, per_file_gcov=False):
    """Run the benchmark on sampled files."""
    files = sample_files(sample_size, benchmark_dir)
    log_path = Path(f"{bname}.log")
    log_file = open(log_path, 'w')

    overall_start_time = time.time()

    print(f"Running benchmark on {sample_size} test files in {benchmark_dir}\n", file=log_file)
    print(f"No. jobs: {job_size}\n", file=log_file)
    print("\n-------------------------------------\n", file=log_file)

    # Run commands either in parallel or sequentially
    if job_size > 1:
        with ThreadPoolExecutor(max_workers=job_size) as executor:
            futures = {executor.submit(process_file, file, cmd_arg): file for file in files}
            for future in as_completed(futures):
                print(future.result(), file=log_file)
    else:
        for file in files:
            log = process_file(file, cmd_arg)
            print(log, file=log_file)

    overall_duration = (time.time() - overall_start_time) * 1000  # Convert to milliseconds

    print("\n-------------------------------------\n", file=log_file)
    print(f"=> Overall Execution Time: {overall_duration:.2f} ms", file=log_file)

    log_file.close()






