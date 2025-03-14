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

clean:
    cd "{{cvc5dir}}/build" && make clean

# Generate a coverage report, use TRACK_UNUSED only if needed, it significantly increases the resulting DB size
bench-measure CORES=num_cpus() DB_FILE="{{reportsdir}}/report.sqlite" EXEC="{{cvc5dir}}/build/bin/cvc5 --tlimit 5000 {}" TRACK_UNUSED="false": build-measure
    ./gen_coverage/target/release/gen_coverage \
        -j {{CORES}} \
        --repo "{{cvc5dir}}" \
        --exec "{{EXEC}}" \
        "{{DB_FILE}}" \
        coverage \
        --benchmarks "{{benchdir}}/**/*.smt2" \
        --track-all {{TRACK_UNUSED}} \
        --coverage-kinds functions \
        --use-prefixes
    @echo "Created report at '{{DB_FILE}}'"

# Evaluate a cvc5 binary
bench-evaluate ID="" CORES=num_cpus() DB_FILE="{{reportsdir}}/report.sqlite" EXEC="{{cvc5dir}}/build/bin/cvc5 --tlimit 5000 {}": build-measure
    ./gen_coverage/target/release/gen_coverage \
        -j {{CORES}} \
        --repo "{{cvc5dir}}" \
        --exec "{{EXEC}}" \
        "{{DB_FILE}}" \
        evaluate \
        --id "{{ID}}"
    @echo "Stored report in '{{DB_FILE}}'"

# Find a solution to our optimization problem
bench-optimize +P_VALUES: build-optimize
    ./benchopt {{P_VALUES}}

# Evaluate solutions to our optimization problem
bench-optimize-eval +SOL_FILES: build-optimize-eval
    ./evaluate_sol {{SOL_FILES}}
        
# Remove rarely used code segments
bench-remover: build-remover
    ./code_remover/target/release/code_remover remove --config ./code_remover/config.toml --no-change false
    @echo "Removed all rarely used functions from code base"

# Find minimal illegal feature example
bench-creduce +SMT_FILES:
    #!/usr/bin/env sh
    export CVC5_TIME_LIMIT="5000"
    export CVC5_BIN="$(realpath '{{cvc5dir}}/build/bin/cvc5')"
    echo "Running creduce on files {{SMT_FILES}} to determine minimal illegal feature (timelimit: $CVC5_TIME_LIMIT, binary: $CVC5_BIN)"
    mkdir -p "creduce/results" "creduce/inputs" "creduce/curr"
    for file in {{SMT_FILES}}; do 
        SMT_RESULT_FILE="bench_reduced_$(date +%s).smt2"
        cp "$file" "creduce/curr/$SMT_RESULT_FILE";
        cd "creduce/curr";
        creduce --n $(nproc) --shaddap --not-c ../interestingness_test.sh "$SMT_RESULT_FILE" && \
        mv "$SMT_RESULT_FILE.orig" "../inputs/$SMT_RESULT_FILE" && \
        mv "$SMT_RESULT_FILE" "../results/$SMT_RESULT_FILE" && \
        echo && echo && echo && \
        echo "Stored minimal bench file at 'creduce/results/$SMT_RESULT_FILE' and original file at 'creduce/inputs/$SMT_RESULT_FILE'" && \
        echo "========================================================";
        cd ../..;
    done;

    echo && echo && echo
    echo "========================================================"
    echo "Stored all minimal bench files at 'creduce/results/' and original file at 'creduce/inputs/'"

    echo && echo && echo
    echo "========================================================"
    echo "Unique results: "
    cat creduce/results/* | sed -e 's/\s\s*/ /g' | sed -e 's/^\s*//' | sed -e 's/\s*$//' | sort | uniq

bench-creduce-results:
    for f in creduce/results/*; do echo; echo "==================================="; echo "$f:"; cat $f; echo "----------------"; ../cvc5-repo/build/bin/cvc5 $f ; done


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

build-cvc5-debug: setup-cvc5
    cd {{cvc5dir}} && ./configure.sh debug --auto-download --pyvenv --poly --cocoa --gpl
    cd "{{cvc5dir}}/build" && make -j $(nproc)

build-cvc5-production: setup-cvc5
    cd {{cvc5dir}} && ./configure.sh production --auto-download --pyvenv --poly --cocoa --gpl
    cd "{{cvc5dir}}/build" && make -j $(nproc)

build-remover: setup-rust
    cd "code_remover" && CARGO_HOME="../{{cargo_env}}" RUSTFLAGS='-C target-cpu=native' ../{{cargo}} build --release

build-measure: setup-rust
    cd "gen_coverage" && CARGO_HOME="../{{cargo_env}}" RUSTFLAGS='-C target-cpu=native' ../{{cargo}} build --release

build-optimize: setup-gurobi
    g++ -o benchopt ./optimization/util.cpp ./optimization/main.cpp -I.gurobi/include -L.gurobi/lib -Wl,-rpath,.gurobi/lib -lgurobi_c++ -lgurobi110 -lsqlite3 -std=c++17 -O3

build-optimize-eval:
    g++ -o evaluate_sol ./optimization/util.cpp ./optimization/evaluate_sol.cpp -I.gurobi/include -L.gurobi/lib -Wl,-rpath,.gurobi/lib -lgurobi_c++ -lgurobi110 -lsqlite3 -std=c++17 -O3

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

