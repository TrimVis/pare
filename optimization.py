#!/usr/bin/env python3.11

import os
import sqlite3
import gurobipy as gp
from gurobipy import GRB


DB_FILE = "./coverage_db.sqlite"
LICENSE_FILE = "./gurobi.lic"
MODEL_NAME = "benchopt"

def main():
    with get_env_from_license(LICENSE_FILE) as env:
        env.start()
        with gp.Model(MODEL_NAME, env=env) as m:
            p = 0.99
            no_benchs = 200
            (n, uids, l, c) = get_function_stats_from_db()

            # F_i, indicates if function i is used or not (in the debloated version)
            f = m.addVars(n, vtype=GRB.INTEGER, lb=0, ub=1, name="F")


            # Minimize the total number of alive code
            m.setObjective(sum(l[i] * f[i] for i in range(n)), GRB.MINIMIZE)

            m.addConstr(sum(c[i] * f[i] for i in range(n)) >=  p * sum(l), "c0")

            # Optimize model
            m.optimize()

            # for v in m.getVars():
            #     print(f"{v.VarName} {v.X:g}")

            print(f"No functions in use: {sum([v.X for v in m.getVars()])}")
            print(f"Total code length:")
            print(f"\tbefore optimization: {sum([l[i] for i in range(n) if c[i]])}")
            print(f"\tafter optimization: {sum([l[i] * f[i].X for i in range(n)])}")

            print(f"Achieved constraint: {sum(c[i] * f[i].X for i in range(n)) } <= {p * sum(l)}")

            print(f"Obj: {m.ObjVal:g}")


def get_function_stats_from_db():
    conn = sqlite3.connect(DB_FILE)
    cur = conn.cursor()

    # Get all entries
    query = f"SELECT * FROM FunctionUsage"
    cur.execute(query)
    rows = cur.fetchall()


    uids = []
    l = []
    c = []
    for (uid, bcount, file, name, start, end) in rows:
        # Filter out include files
        if not file.startswith("src/") and not file.startswith("build/"): 
            continue

        # Filter out functions whose len has not been properly detected
        if end == -1:
            continue

        # All functions will save at least one line
        leng = max(int(end) - int(start), 1)

        uids.append(uid)
        l.append(leng)
        c.append(bcount)

    return (len(uids), uids, l, c)
        



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
