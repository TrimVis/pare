import os
import shutil
import glob
import json

from collections import defaultdict
from pathlib import Path

from .config import GCOV_PREFIX_BASE

def gcov_init():
    shutil.rmtree(GCOV_PREFIX_BASE, ignore_errors=True)
    os.makedirs(GCOV_PREFIX_BASE, exist_ok=True)

def gcov_cleanup():
    shutil.rmtree(GCOV_PREFIX_BASE, ignore_errors=True)

def get_file_uid(file):
    hash = hashlib.sha256(str(file).encode())
    h_readable = ''.join(Path(file).parts[-2:])

    return f"{hash}-{h_readabable}"

def get_prefix(file):
    return GCOV_PREFIX_BASE + get_file_uid(file)

def get_gcov_env(file):
    env = os.environ.copy()
    env["GCOV_PREFIX"] = get_prefix(file)

    return env

def get_gcda_paths():
    path_wildcard = os.path.join(GCOV_PREFIX_BASE, "**/*.gcda")
    return glob.glob(path_wildcard, recursive=True)

def get_prefix_files_map():
    res = defaultdict(list)
    for p in get_gcda_paths():
        parts = Path(p.removeprefix(GCOV_PREFIX_BASE)).parts
        prefix = GCOV_PREFIX_BASE + parts[0]
        file = os.joinpath('/', *parts[1:])
        res[prefix].append(file)

    return res


def gen_json_reports():
    report = { "sources": {} }
    prefix_files_map = get_prefix_files_map()

    for (prefix, files) in prefix_files_map.items():
        env = os.environ.copy()
        env["GCOV_PREFIX"] = prefix

        files_report = { "sources": {} }
        for gcda_file in files:
            result = subprocess.run(['gcov', '--json', '--stdout', gcda_file], env=env, check=True, capture_output=True, text=True)
            next_report = json.loads(result.stdout)

            # Merge files together using our special "per-test" counter
            files_report = combine_reports(files_report, next_report, exec_one=True)

        # Merge files together normally
        report = combine_reports(report, files_report, exec_one=False)

    with open("./coverage.json", "w") as f:
        json.dump(report, f)

