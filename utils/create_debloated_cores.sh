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

    # Prepare repo: restore src, checkout personal/main, and create new branch.
    echo "Restoring ./src..."
    git restore ./src
    echo "Checking out personal/main..."
    git checkout personal/main
    BRANCH="debloated/$P"
    echo "Creating new branch $BRANCH..."
    git checkout -b "$BRANCH"

    echo "Returning to workingd directory for rewrite..."
    cd "$CURR_DIR"
    # Update the config file automatically (in ./code_remover/config.toml)
    CONFIG_PATH="./code_remover/config.toml"
    echo "Updating config file at $CONFIG_PATH with p = $P..."
    cat <<EOF > "$CONFIG_PATH"
db = "./reports/report.sqlite"
p = $P
placeholder = "std::cout << \"Unsupported feature in {func_name} ({file_name})\" << std::endl; exit(1000);"
imports = ["#include <iostream>", "#include <cstdlib>"]
EOF
    echo "Config file updated."

    # Run the rewrite step 
    echo "=================" | tee -a debloating_output.log
    echo "Running rewrite command: ./just r"
    echo "REMOVING FOR P=$P" | tee -a debloating_output.log
    ./just r  | tee -a debloating_output.log

    # Submit repository changes.
    echo "Returning to repository directory for commit..."
    cd "$REPO_DIR"
    echo "Committing debloated codebase changes..."
    git commit -am "Added debloated codebase for p=$P"

    echo "Building binary..."
    cd build && make -j64 && cd ..
    echo "Adding binary for comparison..."
    git add build/bin/cvc5 -f
    echo "Committing binary..."
    git commit -m "Added binary for comparison reasons"
    echo "Pushing branch $BRANCH to remote 'personal'..."
    git push -u personal "$BRANCH"

    echo "Finished processing p = $P."
    echo "-----------------------------------------"

done

# If we reached this point, the script completed successfully.
# Restore the stashed changes.
echo "Restoring stashed changes in repository..."
cd "$REPO_DIR"
# git stash pop

echo "Script completed successfully."

