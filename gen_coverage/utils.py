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

def process_file(file, cmd_arg, gcov_prefix=None):
    """Process a single file with the given command."""
    print(f"[{time.strftime('%Y-%m-%d %H:%M:%S')}] File: {file}")
    res = f"| File: {file}\n"
    start_time = time.time()
    try:
        if gcov_prefix:
            env = os.environ.copy()
            env["GCOV_PREFIX"] = gcov_prefix
            result = subprocess.run(cmd_arg + [file], env=env, check=True, capture_output=True, text=True,)
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

    gcov_prefixes = {}
    if per_file_gcov:
        shutil.rmtree("/tmp/gcov_res/", ignore_errors=True)
        gcov_prefixes = { file: "/tmp/gcov_res/" + str(file).replace('/', '').replace('\\', '') for file in files }
        os.makedirs("/tmp/gcov_res", exist_ok=True)

    # Run commands either in parallel or sequentially
    if job_size > 1:
        with ThreadPoolExecutor(max_workers=job_size) as executor:
            futures = {executor.submit(process_file, file, cmd_arg, gcov_prefixes.get(file, None)): file for file in files}
            for future in as_completed(futures):
                print(future.result(), file=log_file)
    else:
        for file in files:
            log = process_file(file, cmd_arg, gcov_prefixes.get(file, None))
            print(log, file=log_file)

    overall_duration = (time.time() - overall_start_time) * 1000  # Convert to milliseconds

    print("\n-------------------------------------\n", file=log_file)
    print(f"=> Overall Execution Time: {overall_duration:.2f} ms", file=log_file)

    log_file.close()

    return (log_path, gcov_prefixes)


def addDicts(dict1, dict2):
    """Add dicts together by value. i.e. addDicts({"a":1,"b":0}, {"a":2}) == {"a":3,"b":0}."""
    result = {k:v for k,v in dict1.items()}
    for k,v in dict2.items():
        if k in result:
            result[k] += v
        else:
            result[k] = v

    return result

def addLists(list1, list2):
    """Add lists together ignoring value. i.e. addLists([4,1], [2,2,0]) == [2,2]."""
    # Find big list and small list
    blist, slist = list(list2), list(list1)
    if len(list1) > len(list2):
        blist, slist = slist, blist

    # Overlay small list onto big list
    for i, b in enumerate(slist):
        blist[i] += b

    return blist


def combine_reports(base, overlay):
    for source, scov in overlay["sources"].items():
        if source not in base["sources"]:
            base["sources"][source] = {}

        for test_name, tcov in scov.items():
            if test_name not in base["sources"][source]:
                base["sources"][source][test_name] = { "lines": {}, "branches": {}, "functions": {} }

            tcov["lines"] = { k: 1 if v else 0 for k, v in tcov["lines"].items()}
            
            base_data = base["sources"][source][test_name]
            base_data["lines"] = addDicts(base_data["lines"], tcov["lines"])

            for branch, cov in tcov["branches"].items():
                cov = [ 1 if c else 0 for c in cov ]

                if branch not in base_data["branches"]:
                    base_data["branches"][branch] = cov
                else:
                    base_data["branches"][branch] = addLists(base_data["branches"][branch], cov)

            for function, cov in tcov["functions"].items():
                cov["execution_count"] = 1 if cov["execution_count"] else 0

                if function not in base_data["functions"]:
                    base_data["functions"][function] = cov
                else:
                    base_data["functions"][function]["execution_count"] += cov["execution_count"]


def make_helper(cmd, gcov_prefixes={}, out=os.devnull):
    assert cmd in ["coverage-reset", "coverage-json"] or not gcov_prefixes, "--individual does not support --full-report"
    if gcov_prefixes:
        report = { "sources": {} }
        for prefix in gcov_prefixes.values():
            env = os.environ.copy()
            env["GCOV_PREFIX"] = prefix
            subprocess.run(['make', cmd], stdout=out, stderr=out, env=env, check=True)

            if cmd == "coverage-reset":
                continue

            # Merge files together using our special "per-test" counter
            with open("./coverage.json"): 
                add_report = json.load(f)
            shutil.copy("./coverage.json", f"./coverage_{prefix}.json")
            os.remove("./coverage.json")
            report = combine_reports(report, add_report)
        with open("./coverage.json", "w"):
            json.dump(report, f)
    else:
        subprocess.run(['make', cmd], stdout=out, stderr=out, check=True)




