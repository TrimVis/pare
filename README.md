# Master Thesis - Variant 2
### cvc5 minimal trust core

## Idea
> By testing various problem statements determine a "common code subset".
> This subset will then be used to represent the "cvc5 core", and should be
> a competitive SMT solver, with a highly used core.


## Setup
### Automated Coverage Runs
 1. Clone cvc5-repo
 2. Prepare it for coverage testing (`./configure debug --coverage && cd build && make`)
 3. Download benchmarks used for coverage evaluation later on. E.g.:
    - [SMT-LIB Non-Incremental Benchmark 2024](https://zenodo.org/records/11061097)
    - [SMT-LIB Incremental Benchmark 2024](https://zenodo.org/records/11186591)

### Coverage Run Evaluations
 1. Set up a virtual environment. E.g. `python -m venv venv`
 2. Install dependencies. E.g. `source venv/bin/activate && pip install -r requirements.txt`

## Usage
### Automated Coverage Runs
`benchmark_coverage.sh` is a helper script that will run various benchmarks and create coverage tests on random subset of all given benchmarks.

An example execution could look like the following command:<br/>
`./benchmark_coverage.sh -n "all" -j 64 -b ../cvc5-repo/build/ -a "--tlimit 10000" ./benchmarks/nonincremental_2024.04.23/non-incremental/ ./benchmark_runs/tlimit10000-trace/`

*Command Breakdown*
```bash
./benchmark_coverage.sh \
    -n "1,all" \                                # Run whole coverage testsuite over 1 random and all tests
                                                # (number has to be smaller than the total number of tests available)
    -j 64 \                                     # Parallelize across 64 cores
    -b ../cvc5-repo/build/ \                    # Path to build folder (has to be inside the cvc5 repo)
    -a "--tlimit 10000" \                       # Arguments passed to the cvc5 executable found
    ./benchmarks/nonincremental_2024.04.23/ \   # Path to benchmark files (has to only contain smt-lib files)
    ./benchmark_runs/tlimit10000-trace/         # Path to results folder
```

### Coverage Run Evaluations
`eval_coverage_json.py` is a helper script to evaluate the use of functions, lines and lines inside functions (WIP).
It provides commands to generate CSV files which can then potentially be used later on, as well as some plots for visualization purposes.

> *Arguments*
> Note: Not all of these arguments are available for all sub-commands.<br/>
> More arguments are available, check out --help for more information<br/>
>
> `--input=./coverage.json` The coverage.json file created by `make coverage-json`<br/>
> `--output=./out.csv`      Output file<br/>
> `--src_code=../cvc5-repo/src`    Path to src code folder of the cvc5-repo<br/>
> `--cutoff=x`    Filter out any usage values <= x<br/>
> `--log_scale`   (plot only) Use a log10 scale for the y-axis<br/>
> `--relevance_filter=0.1`   (functions-lines only) Filter out functions for which the line by line change is never smaller than 0.1


#### Functions
CSV Example: `python parse_coverage_json.py func_usage csv --input=./coverage.json --src_code=../cvc5-repo/src/ --cutoff 0 --output out_func.csv`<br/>
Plot Example: `python parse_coverage_json.py func_usage plot --input=./coverage.json --src_code=../cvc5-repo/src/ --cutoff 0 --log_scale`

#### Lines
CSV Example: `python parse_coverage_json.py line_usage csv --input=./coverage.json --src_code=../cvc5-repo/src/ --cutoff 0 --output out_lines.csv`<br/>
Plot Example: `python parse_coverage_json.py line_usage plot --input=./coverage.json --src_code=../cvc5-repo/src/ --cutoff 0 --log_scale`

#### Function Lines
CSV Example: `python parse_coverage_json.py fline_usage csv --input=./coverage.json --src_code=../cvc5-repo/src/ --cutoff 0 --output out_func_lines.csv`<br/>
Plot Example: `python parse_coverage_json.py fline_usage plot --input=./coverage.json --src_code=../cvc5-repo/src/ --cutoff 0 --log_scale --relevance_filter 0.1`


## Notes
### Potential Steps
 1. Determine a good set of example SMT-Lib rules
 2. Use these examples to determine necessary solver code (e.g. parsing, etc.)
    ➡ Helpful cvc5 flags: --parse-only, --preprocess-only
                            --force-logic <...>,
 3. Use these examples to determine essential solver code (i.e. actual solving)
    ➡ Analyze traces to find unused/rarely used code and strip that in some way later on.


### Coverage Notes
 - As we want to minimize the trust base, we should intelligently choose the folder we want to focus on
 - E.g. `theory`, as it contains many of the rewriter and theory specific cases.
 - E.g. `preprocessing`, as it likely contains many edge case rewrites.
 - E.g. `smt`, `expr`, `decision`, as it likely contains much of the core code of the SMT solver.

 - Besides optimizing for rarely used lines of code, it might make more sense to focus on rarely used branches during analysis, 
   and replace these branches through an early termination or similar


### Experiment Notes
1. Generate from coverage reports, files with function usage
2. Generate from coverage reports, files with per function line usage (and cutoffs)
3. Also check incremental-mode

I.e. 
```
    (1) retrieve for each function/line f in the code:
        <file-path>:<filenmae.{h,cpp,c}>:<first-line of f>; #total-function-calls(.)   - generate a ranking #total function calls ASC
    (2) filter out all f where #total-function-calls(f) == 0
```
