
cvc5dir := "../cvc5-repo"
cvc5repo := "git@github.com:TrimVis/master-tiny-cvc5.git"
cvc5git := cvc5dir / ".git"
cvc5build := cvc5dir / "build"

benchurl := "https://zenodo.org/api/records/11061097/files-archive"
benchdir := "benchmarks/nonincremental_2024.04.23/non-incremental/"
reportsdir := "reports"

system_python := "python3"
python := ".venv/bin/python3"

alias b := build
alias g := gen_report
alias e := eval_report
alias o := optimize

# TODO pjordan: Add this
# download_bench:
#     mkdir -p "{{benchdir}}"
#     wget "{{benchurl}}" -o benchmark.tar
#     tar -xf *.tar.zst


# Clones and builds cvc5 with coverage support
build:
    mkdir -p "{{cvc5dir}}"
    if test ! -d "{{cvc5git}}"; then git clone --depth 1 "{{cvc5repo}}" "{{cvc5dir}}"; fi

    cd {{cvc5dir}} && ./configure.sh debug --auto-download --coverage --poly --cocoa --gpl
    cd "{{cvc5dir}}/build" && make -j $(nproc)

coverage-reset:
    cd "{{cvc5dir}}/build" && make coverage-reset

setup:
    #!/usr/bin/env sh
    if test ! -e .venv; then
        {{ system_python }} -m venv .venv; 
        {{ python }} -m pip install --upgrade pip
        {{ python }} -m pip install -r ./requirements.txt
    fi

# Generate a coverage report
gen_report TLIMIT="4000" SAMPLE="all" CORES=num_cpus(): setup
    {{ python }} -m gen_coverage \
        -i \
        -n "{{SAMPLE}}" -j {{CORES}} \
        -b ../cvc5-repo/build/ \
        -a "--tlimit {{TLIMIT}}" \
        "{{benchdir}}" \
        "{{reportsdir}}/tlimit{{TLIMIT}}"
    @echo "Created report at '{{reportsdir}}/tlimit{{TLIMIT}}'"

# Evaluate a coverage report
eval_report COVERAGE_FILE=(reportsdir / "tlimit4000/sall_1_coverage.json"): setup
    {{ python }} eval_coverage_json.py db generate --input={{COVERAGE_FILE}} --src_code={{ cvc5dir }}

# Optimize 
optimize: setup
    {{ python }} optimization.py
