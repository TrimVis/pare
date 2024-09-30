import os
import shutil
import glob
import json
import hashlib
import subprocess

from collections import defaultdict
from pathlib import Path

from . import GCOV_PREFIX_BASE
from .utils import combine_reports
from .fastcov import distillSource

def symlink_gcno_files(build_dir, prefix_dir, verbose=False):
    if not prefix_dir or prefix_dir == "/":
        if verbose:
            print("Empty prefix, early return")
        return

    build_dir = Path(build_dir).absolute()
    prefix_dir = Path(prefix_dir).absolute()
    gcno_files = glob.glob(os.path.join(build_dir, '**', '*.gcno'), recursive=True)

    for f in gcno_files:
        # Create target directory if it doesn't exist
        os.makedirs(str(prefix_dir) + os.path.dirname(f), exist_ok=True)

        target_file = str(prefix_dir) + f
        # Create symlink if it doesn't already exist
        if not os.path.exists(target_file):
            os.symlink(f, target_file)
            if verbose:
                print(f"Created symlink: {target_file} -> {f}")
        else:
            if verbose:
                print(f"Symlink already exists: {target_file}")


def gcov_init(bench_dir):
    shutil.rmtree(GCOV_PREFIX_BASE, ignore_errors=True)
    os.makedirs(GCOV_PREFIX_BASE, exist_ok=True)

    # Also make sure to create the per file dirs
    all_files = glob.glob(os.path.join(bench_dir, "**/*.smt2"), recursive=True)
    for file in all_files:
        dir_path = get_prefix(file)
        os.makedirs(dir_path, exist_ok=True)
        

def gcov_cleanup():
    shutil.rmtree(GCOV_PREFIX_BASE, ignore_errors=True)

def get_file_uid(file):
    hash = hashlib.sha256(str(file).encode()).hexdigest()
    h_readable = ''.join(Path(file).parts[-2:])
    if h_readable.endswith(".smt2"): 
        h_readable = h_readable[:-5]
    if len(h_readable) > 20:
        h_readable = h_readable[-20:]

    return f"{hash}-{h_readable}"

def get_prefix(file):
    return os.path.join(GCOV_PREFIX_BASE, get_file_uid(file))

def get_gcov_env(file):
    env = os.environ.copy()
    env["GCOV_PREFIX"] = get_prefix(file)

    return env

def get_prefix_files(prefix=GCOV_PREFIX_BASE):
    path_wildcard = os.path.join(prefix, "**/*.gcda")
    return glob.glob(path_wildcard, recursive=True)

def process_prefix(prefix, files, verbose=False):
    files_report = { "sources": {} }
    # print(f"process_prefix: len(files) {len(files)}")
    for gcda_file in files:
        env = os.environ.copy()
        env["GCOV_PREFIX"] = prefix
        if verbose:
            print("Gcov GCDA File:" + str(gcda_file))
        result = subprocess.run(['gcov', '--json', '--stdout', gcda_file], env=env, check=False, capture_output=True, text=True)
        next_report = {"sources": {}}

        store_noisy_branches = False
        if verbose:
            print("Gcov Exit Code: " + str(result.returncode))
            print("Gcov Errors: " + str(result.stderr or None))

        source = json.loads(result.stdout)
        for f in source["files"]:
            f_path = gcda_file[:-5] if gcda_file.endswith(".gcda") else gcda_file
            f_path = f_path[len(prefix):] if f_path.startswith(prefix) else f_path
            f["file_abs"] = f_path
            distillSource(f, next_report["sources"], "", store_noisy_branches)

        # Merge files together using our special "per-test" counter
        # print(f"process_prefix: combining reports (exec_one=True): \n curr report:" + str(files_report)[0:100] + '\n------------------------------------------\n next report:' + str(next_report)[0:100])
        combine_reports(files_report, next_report, exec_one=True)

    # print(f"process_prefix: files_report (result): " + str(files_report)[0:200])
    return files_report
