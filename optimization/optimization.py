#!/usr/bin/env python3.11

import os
import sqlite3
import gurobipy as gp
import numpy as np
import math
from gurobipy import GRB


DB_FILE = "./reports/report.sqlite"
LICENSE_FILE = "./optimization/gurobi.lic"
MODEL_NAME = "benchopt"


def main():
    print("Extracting values from DB")
    p = 0.95
    (no_benchs, n, uids, len_c, C, B) = get_function_stats_from_db()

    with get_env_from_license(LICENSE_FILE) as env:
        env.start()
        with gp.Model(MODEL_NAME, env=env) as m:

            print("Preparing optimization")
            O = m.addVars(range(n), vtype=GRB.BINARY, obj=1, name="O")
            z = m.addVars(range(no_benchs), vtype=GRB.BINARY, name="z")

            # print(len(C))
            # print(n)
            # print(len(C[0].tolist()[0]))
            # print(no_benchs)

            # print()
            # print(math.prod([O[i] for i in range(1)]))
            # print()

            # print(
            #     math.prod([
            #         O[j]
            #         for j in range(n)
            #         if C[0, j]
            #     ])
            # )

            # m.setObjective(O.prod(len_c), GRB.MINIMIZE)
            # m.addConstr(GRB.quicksum([
            #     math.prod([
            #         O[j]
            #         for j in range(n)
            #         if B[i, j]
            #     ]) for i in range(no_benchs)]
            # ) >= p * no_benchs, "c0")

            for i in range(no_benchs):
                J_i = [j for j in range(n) if B[i, j]]
                m.addConstrs((z[i] <= O[j]
                             for j in J_i), name=f"c_bench_{i}_upper")
                m.addConstr(
                    z[i] >= gp.quicksum(O[j] for j in J_i) - len(J_i) + 1,
                    name=f"c_bench_{i}_lower"
                )
            m.addConstr(z.sum() >= p * no_benchs, "c0")

            print("Running optimization")
            # Optimize model
            m.optimize()

            # m.computeIIS()
            # m.write("model.ilp")

            if m.status == GRB.OPTIMAL:
                # for v in m.getVars():
                #     print(f"{v.VarName} {v.X:g}")

                print(f"No functions in use: {
                    sum([v.X for v in m.getVars()])}")
                # print("Total code length:")
                # print(f"\tbefore optimization: {
                #     sum([l[i] for i in range(n) if c[i]])}")
                # print(f"\tafter optimization: {
                #     sum([l[i] * O[i].X for i in range(n)])}")

                # print(f"Achieved constraint: {sum(
                #     c[i] * O[i].X for i in range(n))} >= {sum(
                #         O[i].X for i in range(n)) * p * no_benchs}")

                print(f"Obj: {m.ObjVal:g}")
            else:
                status = ("INFEASIBLE" if m.status == GRB.INFEASIBLE else
                          "UNBOUNDED" if m.status == GRB.UNBOUNDED else
                          "INF_OR_UNBD" if m.status == GRB.INF_OR_UNBD else
                          f"Uknown case ({m.status})")
                print(
                    f"Could not find optimal result. Exit status: {status}")


def get_function_stats_from_db():
    conn = sqlite3.connect(DB_FILE)
    cur = conn.cursor()

    query = "SELECT COUNT(1) FROM benchmarks"
    cur.execute(query)
    rows = cur.fetchall()
    no_benchs = min(rows[0][0], 2000)

    # Get all entries
    query = "SELECT id, benchmark_usage_count, name, start_line, end_line FROM functions"
    cur.execute(query)
    rows = cur.fetchall()

    uids = []
    len_c = []
    C = []
    for (uid, bcount, name, start, end) in rows:
        uids.append(uid)
        len_c.append(int(end) - int(start) + 1)
        # Overloaded functions are double counted in the current report db
        li = min(bcount, no_benchs)
        # Mock values for Ci
        Ci = li * [1] + (no_benchs - li) * [0]
        C.append(Ci)

    C = np.matrix(C)
    B = C.transpose()

    return (no_benchs, len(uids), uids, len_c, C, B)


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
