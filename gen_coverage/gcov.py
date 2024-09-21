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

def get_gcda_paths(prefix=GCOV_PREFIX_BASE):
    path_wildcard = os.path.join(prefix, "**/*.gcda")
    return glob.glob(path_wildcard, recursive=True)

def get_prefix_files(prefix=GCOV_PREFIX_BASE):
    res = list()
    for p in get_gcda_paths(prefix):
        # print(f"p: '{str(p)}'")
        # print(f"prefix: '{str(prefix)}'")
        # print(f"p woprefix: {p[len(prefix):] if p.startswith(prefix) else p}")
        file = Path(p[len(prefix):] if p.startswith(prefix) else p)
        # print(f"{file}")
        res.append(str(file))

    return res

def process_prefix(prefix, files, verbose=False):

    files_report = { "sources": {} }
    # print(f"process_prefix: len(files) {len(files)}")
    for gcda_file in files:
        env = os.environ.copy()
        env["GCOV_PREFIX"] = prefix
        print("Gcov GCDA File:" + str(gcda_file))
        print("Gcov GCDA Prefix:" + str(prefix))
        print("Gcov Used Path: " + str(prefix + gcda_file))
        result = subprocess.run(['gcov', '--json', '--stdout', prefix + gcda_file], env=env, check=False, capture_output=True, text=True)
        next_report = {"sources": {}}

        store_noisy_branches = False
        print("Gcov Exit Code: " + str(result.returncode))
        print("Gcov Errors: " + str(result.stderr or None))
        source = json.loads(result.stdout)
        print(f"process_prefix: distilling result:" + str(source)[0:100])
        for f in source["files"]:
            f["file_abs"] = gcda_file[:-5] if gcda_file.endswith(".gcda") else gcda_file
            # print(str(f)[0:140])
            distillSource(f, next_report["sources"], "", store_noisy_branches)

        # Merge files together using our special "per-test" counter
        # print(f"process_prefix: combining reports (exec_one=True): \n curr report:" + str(files_report)[0:100] + '\n------------------------------------------\n next report:' + str(next_report)[0:100])
        combine_reports(files_report, next_report, exec_one=True)

    # print(f"process_prefix: files_report (result): " + str(files_report)[0:200])
    return files_report
