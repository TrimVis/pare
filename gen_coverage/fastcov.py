

def distillFunction(function_raw, functions):
    function_name   = function_raw["name"]
    # NOTE: need to explicitly cast all counts coming from gcov to int - this is because gcov's json library
    # will pass as scientific notation (i.e. 12+e45)
    start_line      = int(function_raw["start_line"])
    execution_count = int(function_raw["execution_count"])
    if function_name not in functions:
        functions[function_name] = {
            "start_line": start_line,
            "execution_count": execution_count
        }
    else:
        functions[function_name]["execution_count"] += execution_count

def emptyBranchSet(branch1, branch2):
    return (branch1["count"] == 0 and branch2["count"] == 0)

def matchingBranchSet(branch1, branch2):
    return (branch1["count"] == branch2["count"])

def filterExceptionalBranches(branches):
    filtered_branches = []
    exception_branch = False
    for i in range(0, len(branches), 2):
        if i+1 >= len(branches):
            filtered_branches.append(branches[i])
            break

        # Filter exceptional branch noise
        if branches[i+1]["throw"]:
            exception_branch = True
            continue

        # Filter initializer list noise
        if exception_branch and emptyBranchSet(branches[i], branches[i+1]) and len(filtered_branches) >= 2 and matchingBranchSet(filtered_branches[-1], filtered_branches[-2]):
            return []

        filtered_branches.append(branches[i])
        filtered_branches.append(branches[i+1])

    return filtered_branches

def distillLine(line_raw, lines, branches, include_exceptional_branches):
    line_number = int(line_raw["line_number"])
    count       = int(line_raw["count"])
    if count <  0:
        if "function_name" in line_raw:
            print("WARN: Ignoring negative count found in '%s'.", line_raw["function_name"])
        else:
            print("WARN: Ignoring negative count.")
        count = 0

    if line_number not in lines:
        lines[line_number] = count
    else:
        lines[line_number] += count

    # Filter out exceptional branches by default unless requested otherwise
    if not include_exceptional_branches:
        line_raw["branches"] = filterExceptionalBranches(line_raw["branches"])

    # Increment all branch counts
    for i, branch in enumerate(line_raw["branches"]):
        if line_number not in branches:
            branches[line_number] = []
        blen = len(branches[line_number])
        glen = len(line_raw["branches"])
        if blen < glen:
            branches[line_number] += [0] * (glen - blen)
        branches[line_number][i] += int(branch["count"])

def distillSource(source_raw, sources, test_name, include_exceptional_branches):
    source_name = source_raw["file_abs"]
    if source_name not in sources:
        sources[source_name] = {
            test_name: {
                "functions": {},
                "branches": {},
                "lines": {}
            }
        }

    for function in source_raw["functions"]:
        distillFunction(function, sources[source_name][test_name]["functions"])

    for line in source_raw["lines"]:
        distillLine(line, sources[source_name][test_name]["lines"], sources[source_name][test_name]["branches"], include_exceptional_branches)

