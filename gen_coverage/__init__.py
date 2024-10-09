import enlighten
import math
import time

EXECUTOR = None
TRACE_MEMUSAGE = False
GCOV_PREFIX_BASE = f"/tmp/gcov_gcdas_{math.floor(time.time()*100)}/"
MIN_JOB_SIZE = 50
PROGRESS_MANAGER = enlighten.get_manager(min_delta=0.5)
