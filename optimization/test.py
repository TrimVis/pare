#!/usr/bin/env python3.11

import sys
import os
import sqlite3
import gurobipy as gp
from gurobipy import GRB

def check_status(m):
    if m.status != GRB.OPTIMAL:
        status = ("INFEASIBLE" if m.status == GRB.INFEASIBLE else
                 "UNBOUNDED" if m.status == GRB.UNBOUNDED else
                 "INF_OR_UNBD" if m.status == GRB.INF_OR_UNBD else
                  f"Uknown case ({m.status})")
        print(f"Could not find optimal result. Exit status: {status}")
        m.computeIIS()
        m.write("model.ilp")
        exit(1)

DB_FILE = "./coverage_db.sqlite"
LICENSE_FILE = "./gurobi.lic"

def main():
    with get_env_from_license(LICENSE_FILE) as env:
        env.start()

        p = 0.95 if len(sys.argv) == 1 else float(sys.argv[1])
        (bench, n, uids, l, c) = get_function_stats_from_db(DB_FILE)
        # (x, k) = find_xk(env, p, bench, n, c)
        # print(f"p = {p}")
        # print(f"x = {x}")
        # print(f"k = {k}")
        # find_fi(env, p, x, k, bench, n, uids, l, c)

        find_sol(env, p, bench, n, uids, l, c)

def find_fi(env, p, x, k, bench, n, uids, l, c):
    with gp.Model("benchopt_findfi", env=env) as m:
        # F_i, indicates if function i is used or not (in the debloated version)
        f = m.addVars(range(n), vtype=GRB.BINARY, obj=1, name="F")

        # objective = f.sum()
        objective = f.prod(l)
        # objective = sum(l[i] * f[i] for i in range(n))
        m.setObjective(objective, GRB.MINIMIZE)

        m.addConstr(f.prod(c) >=  p * k * bench, "c0")
        m.addConstr(f.prod(c) >=  x * sum(c), "c1")
        # m.addConstr(sum(c[i] * f[i] for i in range(n)) >=  p * sum(l), "c0")
        # m.addConstr(sum(c[i] * f[i] for i in range(n)) / n >=  (p * no_benchs) // 1, "c0")
        # m.addConstr(sum(c[i] * f[i] for i in range(n)) >=  p * no_benchs, "c0")
        # m.addConstr(sum(c[i] * f[i] for i in range(n)) / n >=  1, "c0")

        m.optimize()
        check_status(m)

        # for v in m.getVars():
        #     print(f"{v.VarName} {v.X:g}")

        print(f"No functions in use: {sum([v.X for v in m.getVars()])}")
        print(f"Total code length:")
        print(f"\tbefore optimization: {sum([l[i] for i in range(n) if c[i]])}")
        print(f"\tafter optimization: {sum([l[i] * f[i].X for i in range(n)])}")

        print(f"Achieved constraint: {sum(c[i] * f[i].X for i in range(n)) } >= {p * k * bench}")

        print(f"Obj: {m.ObjVal:g}")

def find_xk(env, p, bench, n, c):
    with gp.Model("benchopt_findxk", env=env) as m:
        x = m.addVar(lb=0, ub=1, name="x")
        k = m.addVar(lb=1, ub=n, name="c")

        objective = k * p * bench - x * sum(c)
        m.setObjective(objective, GRB.MINIMIZE)
        m.addConstr(objective >=  0, "o0")

        m.optimize()
        check_status(m)

        return (x.X, k.X)

def find_sol(env, p, bench, n, uids, l, c):
    with gp.Model("benchopt_findsol", env=env) as m:
        m.ModelSense = GRB.MINIMIZE

        # F_i, indicates if function i is used or not (in the debloated version)
        x = m.addVar(lb=0, ub=0.99, name="x")
        k = m.addVar(lb=1, ub=n, name="k")
        f = m.addVars(range(n), vtype=GRB.BINARY, obj=1, name="F")

        # Main Objective
        # objective1 = f.sum()
        objective1 = f.prod(l)
        # objective1 = sum(l[i] * f[i] for i in range(n))
        m.setObjectiveN(objective1, 0)

        m.addConstr(f.prod(c) >=  p * k * bench, "c0")
        m.addConstr(f.prod(c) >=  x * sum(c), "c1")
        # m.addConstr(sum(c[i] * f[i] for i in range(n)) >=  p * sum(l), "c0")
        # m.addConstr(sum(c[i] * f[i] for i in range(n)) / n >=  (p * no_benchs) // 1, "c0")
        # m.addConstr(sum(c[i] * f[i] for i in range(n)) >=  p * no_benchs, "c0")
        # m.addConstr(sum(c[i] * f[i] for i in range(n)) / n >=  1, "c0")

        # Side? Objective
        objective2 = k * p * bench - x * sum(c)
        m.setObjectiveN(objective2, 1)
        m.addConstr(objective2 >=  0, "o0")


        m.optimize()
        check_status(m)

        for v in m.getVars():
            if v.VarName.startswith('F['): continue
            print(f"{v.VarName} {v.X:g}")

        print(f"No functions in use: {sum([v.X for v in m.getVars()])}")
        print(f"Total code length:")
        print(f"\tbefore optimization: {sum([l[i] for i in range(n) if c[i]])}")
        print(f"\tafter optimization: {sum([l[i] * f[i].X for i in range(n)])}")

        print(f"Achieved constraint: {sum(c[i] * f[i].X for i in range(n)) } >= {p * k.X * bench}")

        print(f"Obj: {m.ObjVal:g}")



def get_function_stats_from_db(file):
    conn = sqlite3.connect(file)
    cur = conn.cursor()

    # Get all entries
    query = f"SELECT uid, execution_count, file, func_name, start_line, end_line FROM FunctionUsage"
    cur.execute(query)
    rows = cur.fetchall()


    (bench, uids, l, c) = (0, [], [], [])
    for (uid, bcount, file, name, start, end) in rows:
        # Filter out include files
        if not file.startswith("src/") and not file.startswith("build/"): 
            continue

        bench = max(bcount, bench)
        uids.append(uid)
        l.append(max(int(end) - int(start), 1))
        c.append(bcount)

    return (bench, len(uids), uids, l, c)
        



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
