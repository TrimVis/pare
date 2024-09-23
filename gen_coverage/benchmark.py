import os
import sys
import glob
import time
import shutil
import random
import subprocess
import json
from pathlib import Path
from concurrent.futures import ProcessPoolExecutor, as_completed

from .gcov import get_gcov_env, process_prefix, get_prefix, get_prefix_files, combine_reports, symlink_gcno_files

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
        return random.shuffle(all_files)
    else:
        sample_size_int = int(sample_size)
        if sample_size_int > total_files:
            print(f"Error: Requested sample size ({sample_size_int}) is greater than the total number of files ({total_files}) in the directory.")
            sys.exit(1)
        return random.sample(all_files, sample_size_int)

def process_file(file, cmd_arg, build_dir, batch_id=None, use_prefix=False):
    """Process a single file with the given command."""
    res = f"| File: {file}\n"
    start_time = time.time()
    bid_msg = "" if batch_id is None else f" in batch {batch_id}"

    try:
        if use_prefix:
            result = subprocess.run(cmd_arg + [file], env=get_gcov_env(file), check=True, capture_output=True, text=True,)
        else:
            result = subprocess.run(cmd_arg + [file], check=True, capture_output=True, text=True,)
        sout = result.stdout[:-1]
    except subprocess.CalledProcessError as e:
        if e.returncode == -6:
            sout = "timeout"
        else:
            res += f"Error processing file {file}: {e}\n"
            sout = f"crash (returncode: {e.returncode})"

    res += f"{sout}\n"
    duration = (time.time() - start_time) * 1000  # Convert to milliseconds
    res += f"-> Execution Time: {duration:.2f} ms\n"
    print(f"[{time.strftime('%Y-%m-%d %H:%M:%S')}] Execution of /.../{'/'.join(Path(file).parts[-5:])}{bid_msg}:\n{sout}")

    start_time = time.time()

    prefix = get_prefix(file)
    files = get_prefix_files(prefix)
    symlink_gcno_files(build_dir, prefix)
    files_report = process_prefix(prefix, files)

    # Delete the folder to keep storage available
    shutil.rmtree(prefix)

    duration = (time.time() - start_time) * 1000  # Convert to milliseconds
    res += f"-> Processing Time: {duration:.2f} ms\n"
    print(f"[{time.strftime('%Y-%m-%d %H:%M:%S')}] Processed prefix for /.../{'/'.join(Path(file).parts[-5:])}{bid_msg}")

    return (res, files_report)

def process_file_batch(file_batch, cmd_arg, build_dir, batch_id=None, use_prefix=False):
    log = ""
    report = { "sources": {} }
    for file in file_batch:
        (flog, freport) = process_file(file, cmd_arg, build_dir, batch_id, use_prefix=use_prefix)
        log += flog
        combine_reports(report, freport, exec_one=False)
        print(f"[{time.strftime('%Y-%m-%d %H:%M:%S')}] Combined intermediate results for batch {batch_id}")

    return (log, report)


def run_benchmark(sample_size, benchmark_dir, job_size, cmd_arg, bname, build_dir, use_prefix=False):
    """Run the benchmark on sampled files."""
    report = { "sources": {} }
    files = sample_files(sample_size, benchmark_dir)
    log_path = Path(f"{bname}.log")
    log_file = open(log_path, 'w')

    print(f"Running benchmark on {sample_size} test files in {benchmark_dir}\n", file=log_file)
    print(f"No. jobs: {job_size}\n", file=log_file)
    print("\n-------------------------------------\n", file=log_file)

    # Run commands either in parallel or sequentially
    if job_size > 1:

        file_batches = [files[i::job_size] for i in range(10 * job_size)]

        with ProcessPoolExecutor(max_workers=job_size) as executor:
            # futures = {executor.submit(process_file, file, cmd_arg, build_dir, use_prefix): file for file in files}
            futures = { executor.submit(process_file_batch, batch, cmd_arg, build_dir, batch_id, use_prefix): batch_id 
                        for batch_id, batch in enumerate(file_batches)}
            for i, future in enumerate(as_completed(futures)):
                (log, files_report) = future.result()
                print(log, file=log_file)
                combine_reports(report, files_report, exec_one=False)
                print(f"[{time.strftime('%Y-%m-%d %H:%M:%S')}] Combined intermediate results into main result ({i + 1}/{len(futures)})")

    else:
        for file in files:
            (log, files_report) = process_file(file, cmd_arg, build_dir, None, use_prefix=use_prefix)
            print(log, file=log_file)
            combine_reports(report, files_report, exec_one=False)

    with open(f"{bname}_coverage.json", "w") as f:
        json.dump(report, f)

    log_file.close()






