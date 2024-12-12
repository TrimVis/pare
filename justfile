
cvc5dir := "../cvc5-repo"
cvc5repo := "https://github.com/cvc5/cvc5.git"
cvc5git := cvc5dir / ".git"
cvc5build := cvc5dir / "build"

benchurl := "https://zenodo.org/api/records/11061097/files-archive"
benchdir := "benchmarks/nonincremental_2024.04.23/"
reportsdir := "reports"

cargo := ".cargo/bin/cargo"
cargo_env := ".cargo"

alias b := build
alias m := bench-measure
alias o := bench-optimize
alias r := bench-remover

build: build-cvc5 build-remover build-measure build-optimize
    
setup: setup-cvc5 setup-rust setup-gurobi

tidy:
    rm -rf /tmp/coverage_reports
    cd "{{cvc5dir}}/build" && make coverage-reset

# Generate a coverage report
bench-measure CORES=num_cpus(): build-measure
    ./gen_coverage/target/release/gen_coverage \
        -i -j {{CORES}} \
        --build-dir ../cvc5-repo/build/ \
        --coverage-kinds functions \
        "{{benchdir}}" \
        "{{reportsdir}}/report.sqlite"
    @echo "Created report at '{{reportsdir}}/report.sqlite'"

# Find a solution to our optimization problem
bench-optimize +P_VALUES: build-optimize
    ./benchopt {{P_VALUES}}
        
# Remove rarely used code segments
bench-remover: build-remover
    ./code_remover/target/release/code_remover 
    @echo "Removed all rarely used functions from code base"

download-bench:
    mkdir -p "{{benchdir}}"
    rm -f files-archive
    # Downloading benchmarks, this may take a bit (~4.5GB download)
    wget "{{benchurl}}" --show-progress
    # Unpacking archive
    unzip files-archive -d "{{benchdir}}" && rm files-archive
    # Unpacking compressed test files
    cd "{{benchdir}}" && for file in *.tar.zst; do \
        tar --zstd -xf $file && rm $file; \
    done

# Clones and builds cvc5 with coverage support
build-cvc5: setup-cvc5
    cd {{cvc5dir}} && ./configure.sh debug --auto-download --pyvenv --coverage --poly --cocoa --gpl
    cd "{{cvc5dir}}/build" && make -j $(nproc)

build-remover: setup-rust
    cd "code_remover" && CARGO_HOME="../{{cargo_env}}" RUSTFLAGS='-C target-cpu=native' ../{{cargo}} build --release

build-measure: setup-rust
    cd "gen_coverage" && CARGO_HOME="../{{cargo_env}}" RUSTFLAGS='-C target-cpu=native' ../{{cargo}} build --release

build-optimize: setup-gurobi
    g++ -o benchopt ./optimization/main.cpp -I.gurobi/include -L.gurobi/lib -Wl,-rpath,.gurobi/lib -lgurobi_c++ -lgurobi110 -lsqlite3 -std=c++11

setup-cvc5:
    mkdir -p "{{cvc5dir}}";
    if test ! -d "{{cvc5git}}"; then git clone --depth 1 "{{cvc5repo}}" "{{cvc5dir}}"; fi

setup-gurobi:
    #!/usr/bin/env sh
    # Sets up a local gurobi installation
    if test ! -e .gurobi; then
        mkdir .gurobi;
        cd .gurobi;
        wget https://packages.gurobi.com/11.0/gurobi11.0.3_linux64.tar.gz
        wget https://checksum.gurobi.com/11.0.3/gurobi11.0.3_linux64.tar.gz.md5
        md5sum -c gurobi11.0.3_linux64.tar.gz.md5 || {
            echo "Checksum could not be verified. Retry later again";
            cd ..;
            rm -rf .gurobi;
            exit 1;
        };

        tar xvfz gurobi11.0.3_linux64.tar.gz
        mv gurobi1103/linux64/* .
        rm gurobi11.0.3_linux64.tar.gz
        rm gurobi11.0.3_linux64.tar.gz.md5
        rm -rf gurobi1103
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

