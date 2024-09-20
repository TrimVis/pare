import os
import shutil
import glob
import json
import hashlib
import subprocess

from collections import defaultdict
from pathlib import Path

from .utils import combine_reports
from .config import GCOV_PREFIX_BASE
from .fastcov import distillSource

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
    h_readable = ''.join(Path(file).parts[-2:]).removesuffix(".smt2")
    if len(h_readable) > 20:
        h_readable = h_readable[-20:]

    return f"{hash}-{h_readable}"

def get_prefix(file):
    return os.path.join(GCOV_PREFIX_BASE, get_file_uid(file))

def get_gcov_env(file):
    env = os.environ.copy()
    env["GCOV_PREFIX"] = get_prefix(file)

    return env

def get_gcda_paths(prefix=GCOV_PREFIX_BASE):
    path_wildcard = os.path.join(prefix, "**/*.gcda")
    return glob.glob(path_wildcard, recursive=True)

def get_prefix_files(prefix=GCOV_PREFIX_BASE):
    res = list()
    for p in get_gcda_paths(prefix):
        parts = Path(p.removeprefix(GCOV_PREFIX_BASE)).parts
        prefix = GCOV_PREFIX_BASE + parts[0]
        file = os.path.join('/', *parts[1:])
        res.append(file)

    return res

def process_prefix(prefix, files, verbose=False):
    env = os.environ.copy()
    env["GCOV_PREFIX"] = prefix

    files_report = { "sources": {} }
    for gcda_file in files:
        result = subprocess.run(['gcov', '--json', '--stdout', gcda_file], env=env, check=False, capture_output=True, text=True)
        next_report = {"sources": {}}

        store_noisy_branches = False
        source = json.loads(result.stdout)
        for f in source["files"]:
            f["file_abs"] = gcda_file.removesuffix(".gcda")
            distillSource(f, next_report["sources"], "", store_noisy_branches)

        # Merge files together using our special "per-test" counter
        combine_reports(files_report, next_report, exec_one=True)
    return files_report
