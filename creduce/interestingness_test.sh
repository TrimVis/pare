#!/bin/bash
# creduce interestingness test script for our reduced cvc5 binary.

# Ensure required environment variables are set and a input file is provided.
if [[ -z "$CVC5_BIN" ]]; then
  echo "Error: CVC5_BIN environment variable is not set." >&2
  exit 1
fi

if [[ -z "$CVC5_TIME_LIMIT" ]]; then
  echo "Error: CVC5_TIME_LIMIT environment variable is not set." >&2
  exit 1
fi

# Assume that there is only one file in the folder
# if [[ $# -lt 1 ]]; then
#   echo "Usage: $0 <smt-file>"
#   exit 1
# fi

# Run cvc5 with a timeout.
"$CVC5_BIN" --tlimit "$CVC5_TIME_LIMIT" ./*.smt2

EXIT_CODE="$?"

# Check if our exit code is the expected 1000 or a segfault as a hotfix
if [[ $EXIT_CODE -eq 1000 || $EXIT_CODE -eq 139 ]]; then
  exit 0  # Interesting (illegal/removed feature)
else
  exit 1  # Uninteresting (normal execution)
fi

