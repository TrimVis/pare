import time
import shutil
import subprocess
import json
import math
from pathlib import Path
from concurrent.futures import ProcessPoolExecutor, as_completed

from .gcov import get_gcov_env, process_prefix, get_prefix, get_prefix_files, combine_reports, symlink_gcno_files
from .utils import sample_files
from . import MIN_JOB_SIZE, PROGRESS_MANAGER

def process_file(file, cmd_arg, build_dir, batch_id=None, use_prefix=False, verbose=False):
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
    if verbose: print(f"[{time.strftime('%Y-%m-%d %H:%M:%S')}] Execution of /.../{'/'.join(Path(file).parts[-5:])}{bid_msg}:\n{sout}")

    start_time = time.time()

    prefix = get_prefix(file)
    files = get_prefix_files(prefix)
    symlink_gcno_files(build_dir, prefix)
    files_report = process_prefix(prefix, files)

    # Delete the folder to keep storage available
    shutil.rmtree(prefix)

    duration = (time.time() - start_time) * 1000  # Convert to milliseconds
    res += f"-> Processing Time: {duration:.2f} ms\n"
    if verbose: print(f"[{time.strftime('%Y-%m-%d %H:%M:%S')}] Processed prefix for /.../{'/'.join(Path(file).parts[-5:])}{bid_msg}")

    return (res, files_report)

def process_file_batch(file_batch, cmd_arg, build_dir, batch_id=None, use_prefix=False, verbose=False):
    log = ""
    report = { "sources": {} }
    fb_len = len(file_batch)
    for (i, file) in enumerate(file_batch):
        (flog, freport) = process_file(file, cmd_arg, build_dir, batch_id, use_prefix=use_prefix)
        log += flog
        combine_reports(report, freport, exec_one=False)
        if verbose: print(f"[{time.strftime('%Y-%m-%d %H:%M:%S')}] Combined intermediate results for batch {batch_id}")
        if i != fb_len and i % 5 == 4: 
            print(f"[{time.strftime('%Y-%m-%d %H:%M:%S')}] Batch {batch_id}: Processed file {i + 1} of {fb_len}")

    return (log, report, fb_len)


def run_benchmark(sample_size, benchmark_dir, job_size, cmd_arg, bname, build_dir, use_prefix=False, verbose=False):
    """Run the benchmark on sampled files."""
    report = { "sources": {} }
    print(f"[{time.strftime('%Y-%m-%d %H:%M:%S')}] Retrieving files to be benchmarked... (file count: {sample_size})")
    files = sample_files(sample_size, benchmark_dir)
    log_path = Path(f"{bname}.log")
    log_file = open(log_path, 'w')

    log_file.write(f"Running benchmark on {sample_size} test files in {benchmark_dir}\n")
    log_file.write("\n-------------------------------------\n")
    
    print(f"[{time.strftime('%Y-%m-%d %H:%M:%S')}] Starting benchmarks...")

    start_time = time.time()

    # Run commands either in parallel or sequentially
    if job_size > 1:
        batch_size = max(job_size, math.ceil(len(files) / MIN_JOB_SIZE))
        file_batches = [files[i::batch_size] for i in range(batch_size)]
        pbar = PROGRESS_MANAGER.counter(total=len(file_batches), desc='Processing batches', unit='batches')

        with ProcessPoolExecutor(max_workers=job_size) as executor:
            print(f"[{time.strftime('%Y-%m-%d %H:%M:%S')}] Processing of {len(file_batches)} batches (exp. batch_size: {len((file_batches[0:1] or [])[0])}) in {job_size} processes starts now...")
            futures = { executor.submit(process_file_batch, batch, cmd_arg, build_dir, batch_id, use_prefix): batch_id 
                        for batch_id, batch in enumerate(file_batches)}
            future_len = len(futures)
            for i, future in enumerate(as_completed(futures)):
                (log, files_report, batch_size) = future.result()
                log_file.write(log + '\n')
                combine_reports(report, files_report, exec_one=False)
                pbar.update()
                print(f"[{time.strftime('%Y-%m-%d %H:%M:%S')}] Finished batch {i + 1} of {future_len} (batch_size: {batch_size})")

    else:
        pbar = PROGRESS_MANAGER.counter(total=len(files), desc='Processing files', unit='files')
        for file in files:
            (log, files_report) = process_file(file, cmd_arg, build_dir, None, use_prefix=use_prefix)
            log_file.write(log + '\n')
            combine_reports(report, files_report, exec_one=False)
            pbar.update()

    with open(f"{bname}_coverage.json", "w") as f:
        json.dump(report, f)

    duration = (time.time() - start_time)
    msg = f"=> Total Benchmark Runtime: {duration:.2f}s"
    log_file.write(msg + '\n')
    print(f"\n[{time.strftime('%Y-%m-%d %H:%M:%S')}] {msg}")
    if job_size > 1:
        msg = f"=> Avg. Benchmark Runtimes: {(duration / len(files)):.2f} s/file {duration / len(file_batches):.2f} s/batch"
        log_file.write(msg + '\n')
        print(f"[{time.strftime('%Y-%m-%d %H:%M:%S')}] {msg}")
    else:
        msg = f"=> Avg. Runtime: {(duration / len(files)):.2f} s/file"
        log_file.write(msg + '\n')
        print(f"[{time.strftime('%Y-%m-%d %H:%M:%S')}] {msg}")


    log_file.flush()
    log_file.close()






