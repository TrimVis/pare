import hashlib
from pathlib import Path

def combine_dicts(dict1, dict2):
    """Add dicts together by value. i.e. addDicts({"a":1,"b":0}, {"a":2}) == {"a":3,"b":0}."""
    result = {k:v for k,v in dict1.items()}
    for k,v in dict2.items():
        if k in result:
            result[k] += v
        else:
            result[k] = v

    return result

def combine_lists(list1, list2):
    """Add lists together ignoring value. i.e. addLists([4,1], [2,2,0]) == [2,2]."""
    # Find big list and small list
    blist, slist = list(list2), list(list1)
    if len(list1) > len(list2):
        blist, slist = slist, blist

    # Overlay small list onto big list
    for i, b in enumerate(slist):
        blist[i] += b

    return blist


def combine_reports(base, overlay, exec_one=False):
    for source, scov in overlay["sources"].items():
        if source not in base["sources"]:
            if exec_one:
                base["sources"][source] = {}
            else:
                base["sources"][source] = scov
                continue

        for test_name, tcov in scov.items():
            if test_name not in base["sources"][source]:
                base["sources"][source][test_name] = { "lines": {}, "branches": {}, "functions": {} }

            if exec_one:
                tcov["lines"] = { k: 1 if v else 0 for k, v in tcov["lines"].items()}
            
            base_data = base["sources"][source][test_name]
            base_data["lines"] = combine_dicts(base_data["lines"], tcov["lines"])

            for branch, cov in tcov["branches"].items():
                if exec_one:
                    cov = [ 1 if c else 0 for c in cov ]

                if branch not in base_data["branches"]:
                    base_data["branches"][branch] = cov
                else:
                    base_data["branches"][branch] = combine_lists(base_data["branches"][branch], cov)

            for function, cov in tcov["functions"].items():
                if exec_one:
                    cov["execution_count"] = 1 if cov["execution_count"] else 0

                if function not in base_data["functions"]:
                    base_data["functions"][function] = cov
                else:
                    base_data["functions"][function]["execution_count"] += cov["execution_count"]
