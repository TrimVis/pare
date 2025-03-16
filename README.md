# Pare: Data-Driven Life-Code Debloating

Pare is a project focused on identifying and removing rarely used or unnecessary code segments from the cvc5 SMT solver using data-driven approaches. By analyzing execution traces and optimizing benchmarks, Pare aims to create a minimal and efficient core solver while preserving key functionality.

## Features
- **Automated Benchmarking**: Run SMT-LIB benchmarks to evaluate cvc5 performance and coverage.
- **Coverage Analysis**: Identify frequently and infrequently used code sections.
- **Code Reduction**: Remove redundant or rarely executed code to streamline execution.
- **Optimization Framework**: Tune solver configurations based on empirical data.
- **Automation via Justfile**: Simplified task management with predefined `just` commands.

## Setup

### Prerequisites
1. Clone the Pare repository.
2. Ensure required dependencies are installed or available:
   - Bash
   - curl
   - cvc5 Requirements (python, cmake, gcc) 
   - Gurobi License File (if using optimization modules)

### Initial Setup
Run the following commands to set up the environment:
```sh
./download_just.sh # Can be skipped if just is installed system-wide
./just setup
./just build
./just build-cvc5
./just download-bench
```

## Usage
To get a overview of the commands available, run `./just --list`

### Step 1: Data Aggregation
To collect initial execution traces of cvc5 run:
```sh
./just a
```

This command executes benchmarks, tracks function usage, and stores results in an SQLite database at `./reports/report.sqlite` per default.
It is quite configurable, see `./just --list`.

### Step 2: Optimization
To find rarely used functions:
```sh
just o +P_VALUES
```
E.g.
```sh
./just o 0.99 0.95
```

To evaluate optimization results and get an estimate on potential rarely used code savings, run:
```sh
./just bench-optimize-eval +SOL_FILES
```

### Step 3: Code Reduction
To remove unused or rarely used code paths, run:
```sh
./just r
```

Before doing so make sure to configure it, by editing `./code_remover/config.toml`:
```toml
db = "./reports/report.sqlite"
p = 0.75 # Adapt this accordingly

placeholder = "std::cout << \"Unsupported feature\" << std::endl; exit(1000);"
imports = ["#include <iostream>", "#include <cstdlib>"]

[replace_path_prefix]
"/local/home/.../" = "../../" # Useful if you ran the data aggregation step on another machine 
```

### Step 4: Evaluation
To evaluate the current cvc5 binary run:
```sh
./just e
```

This command executes benchmarks, and stores execution results in an SQLite database at `./reports/report.sqlite` per default.
It is quite configurable, see `./just --list`.

### Additional Commands
- **Building cvc5 with coverage support**:
  ```sh
  just build-cvc5
  ```
- **Cleaning the workspace**:
  ```sh
  just clean
  ```

## Research Goals
Pare follows a structured approach to reduce the solver's trusted code base:
1. Identify essential code paths using empirical data.
2. Reduce unnecessary code without affecting correctness.
3. Optimize solver performance through data-driven insights.
4. Automate analysis and evaluation processes for reproducibility.

Pare enables systematic and data-driven approaches to code simplification in SMT solvers, reducing complexity while maintaining performance and correctness.
