
cvc5dir := "../cvc5-repo"
cvc5repo := "https://github.com/cvc5/cvc5.git"
cvc5git := cvc5dir / ".git"
cvc5build := cvc5dir / "build"

benchurl := "https://zenodo.org/api/records/11061097/files-archive"
benchdir := "benchmarks/nonincremental_2024.04.23/"
reportsdir := "reports"

system_python := "python3"
python := ".venv/bin/python3"
cargo := ".cargo/bin/cargo"
cargo_env := ".cargo"

alias b := build
alias g := gen_report
alias o := optimize

download_bench:
    mkdir -p "{{benchdir}}"
    rm files-archive
    # Downloading benchmarks, this may take a bit (~4.5GB download)
    wget "{{benchurl}}" --show-progress
    # Unpacking archive
    unzip files-archive -d "{{benchdir}}" && rm files-archive
    # Unpacking compressed test files
    cd "{{benchdir}}" && for file in *.tar.zst; do \
        tar --zstd -xf $file && rm $file; \
    done


# Clones and builds cvc5 with coverage support
build-cvc5:
    mkdir -p "{{cvc5dir}}"
    if test ! -d "{{cvc5git}}"; then git clone --depth 1 "{{cvc5repo}}" "{{cvc5dir}}"; fi

    cd {{cvc5dir}} && ./configure.sh debug --auto-download --coverage --poly --cocoa --gpl
    cd "{{cvc5dir}}/build" && make -j $(nproc)

build-rust: setup-rust
    cd "gen_coverage" && CARGO_HOME="../{{cargo_env}}" RUSTFLAGS='-C target-cpu=native' ../{{cargo}} build --release

build: build-cvc5 build-rust

tidy:
    rm -rf /tmp/coverage_reports
    cd "{{cvc5dir}}/build" && make coverage-reset

setup-python:
    #!/usr/bin/env sh
    if test ! -e .venv; then
        {{ system_python }} -m venv .venv; 
        {{ python }} -m pip install --upgrade pip
        {{ python }} -m pip install -r ./optimization/requirements.txt
        {{ python }} -m pip install -r ./code_remover/requirements.txt
    fi

setup-rust:
    #!/usr/bin/env sh
    # Sets up a local rust installation (portability reasons)
    if test ! -e .cargo; then
        export CARGO_HOME="{{cargo_env}}";
        export RUSTUP_HOME="{{cargo_env}}" ;
        curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs > rustup_installer.sh;
        bash ./rustup_installer.sh -y --no-modify-path --no-update-default-toolchain;
        rm rustup_installer.sh
    fi

setup: setup-python setup-rust

# Generate a coverage report
gen_report CORES=num_cpus(): build-rust
    ./gen_coverage/target/release/gen_coverage \
        -i -j {{CORES}} \
        --build-dir ../cvc5-repo/build/ \
        --coverage-kinds functions \
        "{{benchdir}}" \
        "{{reportsdir}}/report.sqlite"
    @echo "Created report at '{{reportsdir}}/report.sqlite'"

# Optimize 
optimize: setup
    {{ python }} optimization/optimization.py
