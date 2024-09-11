#!/usr/bin/env python

import fire
import orjson
from loguru import logger 

def _common_prefix(strings):
    if not strings:
        return ""

    strings.sort()

    (i, first, last) = (0, strings[0], strings[-1])
    while i < len(first) and i < len(last) and first[i] == last[i]:
        i += 1

    return first[:i]

MARKER_FILE_NAME = "| File: "
MARKER_EXEC_TIME = "\\-> Execution Time: "
MARKERS_RESULT = ["sat", "unsat", "unknown"]

def _parse_log_file(file: str):
    file_names = []
    # Find a common prefix and strip it
    with open(file, "r") as f:
        for line in f:
            if line.startswith(MARKER_FILE_NAME):
                file_name = line.removeprefix(MARKER_FILE_NAME).strip()
                file_names.append(file_name)
    filename_prefix = _common_prefix(file_names)

    result_map = {}
    with open(file, "r") as f:
        (file_name, exec_time, exec_res) = (None, None, None)
        for line in f:
            if line.startswith(MARKER_FILE_NAME):
                file_name = line.removeprefix(MARKER_FILE_NAME).strip().removeprefix(filename_prefix)
                exec_time = None
                exec_res = "timeout"
            elif line.strip() in MARKERS_RESULT:
                exec_res = line.strip()
            elif line.startswith(MARKER_EXEC_TIME):
                exec_time = line.strip() \
                    .removeprefix(MARKER_EXEC_TIME) \
                    .removesuffix("ms").strip()
                exec_time = int(exec_time)

                # Remember results
                result_map[file_name] = {"time_ms": exec_time, "res": exec_res}

    return result_map


def analyze(log_file: str, output: str):
    logger.info(f"Analyzing log file: {log_file}")
    json_data = orjson.dumps(
        _parse_log_file(log_file)
    )
    logger.info(f"Writing analysis results to: {output}")
    with open(output, 'wb') as f:
        f.write(json_data)


def diff(log0: str, log1: str, verbose=False):
    error_count = 0
    warn_count = 0

    logger.info(f"Analyzing log file: {log0}")
    pl0 = _parse_log_file(log0)
    logger.info(f"Analyzing log file: {log1}")
    pl1 = _parse_log_file(log1)

    k0 = set(pl0.keys())
    k1 = set(pl1.keys())

    print()
    logger.info("Analyzing result difference between runs")
    for k in (k0 & k1):
        r0 = pl0[k]["res"]
        r1 = pl1[k]["res"]
        if r0 != r1:
            if verbose:
                logger.warning(f"Difference found ({k}):")
                logger.warning(f"{log0}: {r0}")
                logger.warning(f"{log1}: {r1}")
            warn_count += 1

            if (r0 == "sat" and r1 == "unsat") or (r1 == "sat" and r0 == "unsat"):
                logger.error(f"Contradicting results found ({k})!")
                error_count += 1

    print()
    logger.info("Analyzing file differences between runs")
    key_diff = len(k0 | k1) - len(k0 & k1)
    if not len(k0 & k1):
        logger.error("No common test files between logs found...")
        error_count += 1
    elif key_diff > 0:
        logger.warning("Only some common test files between logs...")
        logger.warning(f"A total of {key_diff} test files are unique between logs")
        warn_count += 1
    else:
        logger.info("All test files are common between log files!")

    print()
    print()
    if error_count:
        logger.critical(f"Encountered a total of {error_count} critical errors and {warn_count} warnings")
        exit(1)
    else:
        logger.info(f"Encountered no critical errors and {warn_count} warnings")
    logger.info("Use --verbose to see all warnings")



if __name__ == "__main__":
    fire.Fire()
