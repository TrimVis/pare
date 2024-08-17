# Master Thesis - Variant 2
## cvc5 minimal trust core

### Idea
> By testing various problem statements determine a "common code subset".
> This subset will then be used to represent the "cvc5 core", and should be
> a competitive SMT solver, with a highly used core.

### Potential Steps
 1. Determine a good set of example SMT-Lib rules
 2. Use these examples to determine necessary solver code (e.g. parsing, etc.)
    --> Helpful cvc5 flags: --parse-only, --preprocess-only
                            --force-logic <...>,
 3. Use these examples to determine essential solver code (i.e. actual solving)
    --> Analyze traces to find unused/rarely used code and strip that in some way later on.


### Feasibility Experiment
 - Function Coverage (w & w/o parse-only, preprocess-only) of Benchmarks
    --> Estimate "live code"
    --> Sample 40/400/4000 tests and track runtime trend (with & w/o tracing enabled)
 -


### Rarely Used Code Notes
#### General Notes
 - As we want to minimize the trust base, we should intelligently choose the folder we want to focus on
 - E.g. `theory`, as it contains many of the rewriter and theory specific cases.
 - E.g. `preprocessing`, as it likely contains many edge case rewrites.
 - E.g. `smt`, `expr`, `decision`, as it likely contains much of the core code of the SMT solver.

 - Besides optimizing for unused lines of code, it might make more sense to focus on unused branches during analysis, 
   and replace these branches through an early termination or similar


#### Code Analysis
##### Unused/Dead
 - `printer/ast` (208)
 - `proof/dot` (242)
 - `proof/alf` (938)
 - `rewriter` (1250)
 - `proof/alethe` (1553)
 - `proof/lfsc` (1796)
 - `api/c` (3745)

##### Rarely Used
 - `theory/arith/nl/icp` (401)
    - Mostly unused
    - solver and reset used
 - `proof` (3099)
    - Mostly unused
    - except for eager_proof_gen and proof_rule_checker
 - `theory/sets` (4596)
    - even lower branch coverage
 - `point` (lines)
    - point1
 - `point` (lines)
    - point1
 - `point` (lines)
    - point1
 - `point` (lines)
    - point1
 - `point` (lines)
    - point1
 - `point` (lines)
    - point1
 - `point` (lines)
    - point1
 - `point` (lines)
    - point1
 - `point` (lines)
    - point1
 - `point` (lines)
    - point1
 - `point` (lines)
    - point1
 - `point` (lines)
    - point1
 - `point` (lines)
    - point1
 - `point` (lines)
    - point1
 - `point` (lines)
    - point1
 - `point` (lines)
    - point1
 - `point` (lines)
    - point1
 - `point` (lines)
    - point1
 - `point` (lines)
    - point1
 - `proof`
    - Mostly unused


##### Live Code
