#!/usr/bin/env bash
set -euo pipefail

if [ "$#" -lt 1 ]; then
    echo "Usage: $0 <p_value1> [<p_value2> ...]"
    exit 1
fi

# Save the current directory (where the rewrite command will be executed)
CURR_DIR="$(pwd)"

# Define the repository directory (assumed to be a sibling directory)
REPO_DIR="../cvc5-repo"

cp ./reports/report.sqlite ./reports/report_eval.sqlite

# Change to the repository directory and stash any local changes.
echo "Changing directory to repository: $REPO_DIR"
cd "$REPO_DIR"
echo "Stashing local changes..."
git stash push -u -m "Pre-debloating stash" || { echo "Stash failed!"; exit 1; }
STASH_CREATED=true

# Process each provided p value.
for P in "$@"; do
    echo "Processing p = $P"
    export P

    BRANCH="debloated/$P"
    echo "Checking out $BRANCH..."
    git checkout "$BRANCH"

    echo "Returning to workingd directory for build..."
    cd "$CURR_DIR"
    ./just build-cvc5-production
    echo "Starting evaluation" 
    # ./just bench-evaluate "$P" 190 ./reports/report_eval.sqlite
    ./just bench-evaluate "$P" 190 ./reports/report_eval.sqlite "../cvc5-repo/build/bin/cvc5 --tlimit 5000 {}"
    cd "$REPO_DIR"

done

echo "Script completed successfully."

