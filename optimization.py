#!/usr/bin/env python3.11

import os
import sqlite3
import gurobipy as gp
from gurobipy import GRB


DB_FILE = "./coverage_db.sqlite"
LICENSE_FILE = "./gurobi.lic"
MODEL_NAME = "benchopt"


# New Idea: 
#    Estimate the normalization factor c, based on the distribution of C_i
#    --> Assume a gaussian distribution (natural dataset)
#    --> Assume that f_i with large C_i, are likely to have a large overlap
# 
# Both of these have to be shown for our dataset, additionally it would probably
# be good to add some asserts to the code, that try to verify both of these assumptions
# (2nd probably not easily verifiable, but logical)

def main():
    with get_env_from_license(LICENSE_FILE) as env:
        env.start()
        with gp.Model(MODEL_NAME, env=env) as m:
            p = 0.95
            (no_benchs, n, uids, l, c) = get_function_stats_from_db()

            # F_i, indicates if function i is used or not (in the debloated version)
            f = m.addVars(range(n), vtype=GRB.BINARY, obj=1, name="F")


            m.setObjective(f.prod(l), GRB.MINIMIZE)
            # m.addConstr(f.prod(c) >=  f.sum() * (p * no_benchs), "c0")

            for j in range(no_benchs):
                functions_in_benchmark = get_functions_in_benchmark(j)
                m.addConstr(B[j] <= quicksum(f[i] for i in functions_in_benchmark), f"benchmark_{j}")
            m.addConstr(B.sum() >= p * no_benchs, "coverage_constraint")


            # Minimize the total number of alive code
            # m.setObjective(sum(l[i] * f[i] for i in range(n)), GRB.MINIMIZE)
            # m.setObjective(sum(f[i] for i in range(n)), GRB.MINIMIZE)

            # m.addConstr(sum(c[i] * f[i] for i in range(n)) >=  p * sum(l), "c0")
            # m.addConstr(sum(c[i] * f[i] for i in range(n)) / n >=  (p * no_benchs) // 1, "c0")
            # m.addConstr(sum(c[i] * f[i] for i in range(n)) >=  p * no_benchs, "c0")

            # m.addConstr(sum(c[i] * f[i] for i in range(n)) / n >=  1, "c0")

            # Optimize model
            m.optimize()

            # m.computeIIS()
            # m.write("model.ilp")

            if m.status == GRB.OPTIMAL:
                # for v in m.getVars():
                #     print(f"{v.VarName} {v.X:g}")

                print(f"No functions in use: {sum([v.X for v in m.getVars()])}")
                print(f"Total code length:")
                print(f"\tbefore optimization: {sum([l[i] for i in range(n) if c[i]])}")
                print(f"\tafter optimization: {sum([l[i] * f[i].X for i in range(n)])}")

                print(f"Achieved constraint: {sum(c[i] * f[i].X for i in range(n)) } >= {sum(f[i].X for i in range(n)) * p * no_benchs}")

                print(f"Obj: {m.ObjVal:g}")
            else:
                status = ("INFEASIBLE" if m.status == GRB.INFEASIBLE else
                         "UNBOUNDED" if m.status == GRB.UNBOUNDED else
                         "INF_OR_UNBD" if m.status == GRB.INF_OR_UNBD else
                          f"Uknown case ({m.status})")
                print(f"Could not find optimal result. Exit status: {status}")

def get_function_stats_from_db():
    conn = sqlite3.connect(DB_FILE)
    cur = conn.cursor()

    # Get all entries
    query = f"SELECT uid, execution_count, file, func_name, start_line, end_line FROM FunctionUsage"
    cur.execute(query)
    rows = cur.fetchall()


    no_benchs = 0
    uids = []
    l = []
    c = []
    counter = 0
    for (uid, bcount, file, name, start, end) in rows:
        counter += 1
        # Filter out include files
        if not file.startswith("src/") and not file.startswith("build/"): 
            continue

        no_benchs = max(bcount, no_benchs)
        uids.append(uid)
        leng = max(int(end) - int(start), 1)
        l.append(leng)
        c.append(bcount)

    return (no_benchs, len(uids), uids, l, c)
        



def get_env_from_license(file_path):
    if not os.path.exists(file_path):
        return gp.Env()
    env = gp.Env(empty=True)

    with open(file_path, 'r') as file:
        for line in file:
            # Check if the line contains any of the required keys
            if line.startswith("WLSACCESSID"):
                aid = line.split('=')[1].strip()
                env.setParam("WLSAccessID", aid)
            elif line.startswith("WLSSECRET"):
                secret = line.split('=')[1].strip()
                env.setParam("WLSSecret", secret)
            elif line.startswith("LICENSEID"):
                lid = int(line.split('=')[1].strip())
                env.setParam("LicenseID", lid)

    return env


if __name__ == "__main__":
    try:
        main()
    except gp.GurobiError as e:
        print(f"Error code {e.errno}: {e}")
    except AttributeError:
        print("Encountered an attribute error")
