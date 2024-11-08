#include "gurobi_c++.h"
#include <algorithm> // For std::replace
#include <cmath>
#include <cstdlib>
#include <fstream>
#include <iomanip> // For std::setprecision
#include <iostream>
#include <sqlite3.h>
#include <sstream>
#include <string>
#include <vector>

const std::string DB_FILE = "./reports/report.sqlite";
const std::string LICENSE_FILE = "./optimization/gurobi.lic";
const std::string MODEL_NAME = "benchopt";

// FIXME: Due to local hardware constraints and for testing purposes
// I limit the no of benches being analyzed to a subset
const float SCALER = 0.005;

void store_used_functions_to_db(std::vector<bool> &func_state,
                                std::vector<int> &func_ids, float p) {
  sqlite3 *db;
  int rc = sqlite3_open(DB_FILE.c_str(), &db);
  if (rc) {
    std::cerr << "Can't open database: " << sqlite3_errmsg(db) << std::endl;
    exit(1);
  }

  // Create the table name using 'p', replacing any '.' with '_'
  std::ostringstream table_name_stream;
  table_name_stream << "optimization_result_p" << std::fixed
                    << std::setprecision(4) << p;
  std::string table_name = table_name_stream.str();
  std::replace(table_name.begin(), table_name.end(), '.',
               '_'); // Replace '.' with '_'

  // Create the table
  std::ostringstream create_table_stream;
  create_table_stream << "CREATE TABLE " << table_name
                      << " (func_id INTEGER, use_function INTEGER);";
  std::string create_table_query = create_table_stream.str();

  rc = sqlite3_exec(db, create_table_query.c_str(), nullptr, nullptr, nullptr);
  if (rc != SQLITE_OK) {
    std::cerr << "Failed to create table: " << sqlite3_errmsg(db) << std::endl;
    sqlite3_close(db);
    exit(1);
  }

  // Prepare the INSERT statement
  std::ostringstream insert_stream;
  insert_stream << "INSERT INTO " << table_name
                << " (func_id, use_function) VALUES (?, ?);";
  std::string insert_query = insert_stream.str();

  sqlite3_stmt *insert_stmt;
  rc = sqlite3_prepare_v2(db, insert_query.c_str(), -1, &insert_stmt, nullptr);
  if (rc != SQLITE_OK) {
    std::cerr << "Failed to prepare insert statement: " << sqlite3_errmsg(db)
              << std::endl;
    sqlite3_close(db);
    exit(1);
  }

  // Begin transaction for efficiency
  rc = sqlite3_exec(db, "BEGIN TRANSACTION;", nullptr, nullptr, nullptr);
  if (rc != SQLITE_OK) {
    std::cerr << "Failed to begin transaction: " << sqlite3_errmsg(db)
              << std::endl;
    sqlite3_finalize(insert_stmt);
    sqlite3_close(db);
    exit(1);
  }

  // Loop over the data and insert into the table
  for (size_t i = 0; i < func_ids.size(); ++i) {
    // Bind func_id
    rc = sqlite3_bind_int(insert_stmt, 1, func_ids[i]);
    if (rc != SQLITE_OK) {
      std::cerr << "Failed to bind func_id: " << sqlite3_errmsg(db)
                << std::endl;
      sqlite3_finalize(insert_stmt);
      sqlite3_close(db);
      exit(1);
    }

    // Bind use_function (convert bool to integer 0 or 1)
    rc = sqlite3_bind_int(insert_stmt, 2, func_state[i] ? 1 : 0);
    if (rc != SQLITE_OK) {
      std::cerr << "Failed to bind use_function: " << sqlite3_errmsg(db)
                << std::endl;
      sqlite3_finalize(insert_stmt);
      sqlite3_close(db);
      exit(1);
    }

    // Execute the INSERT statement
    rc = sqlite3_step(insert_stmt);
    if (rc != SQLITE_DONE) {
      std::cerr << "Failed to execute insert statement: " << sqlite3_errmsg(db)
                << std::endl;
      sqlite3_finalize(insert_stmt);
      sqlite3_close(db);
      exit(1);
    }

    // Reset the statement for the next iteration
    rc = sqlite3_reset(insert_stmt);
    if (rc != SQLITE_OK) {
      std::cerr << "Failed to reset insert statement: " << sqlite3_errmsg(db)
                << std::endl;
      sqlite3_finalize(insert_stmt);
      sqlite3_close(db);
      exit(1);
    }
  }

  // Commit the transaction
  rc = sqlite3_exec(db, "COMMIT;", nullptr, nullptr, nullptr);
  if (rc != SQLITE_OK) {
    std::cerr << "Failed to commit transaction: " << sqlite3_errmsg(db)
              << std::endl;
    sqlite3_finalize(insert_stmt);
    sqlite3_close(db);
    exit(1);
  }

  // Clean up
  sqlite3_finalize(insert_stmt);
  sqlite3_close(db);
}

void get_function_stats_from_db(int &no_benchs, int &n, std::vector<int> &uids,
                                std::vector<int> &len_c,
                                std::vector<std::vector<bool>> &C,
                                std::vector<std::vector<bool>> &B) {
  sqlite3 *db;
  int rc = sqlite3_open(DB_FILE.c_str(), &db);
  if (rc) {
    std::cerr << "Can't open database: " << sqlite3_errmsg(db) << std::endl;
    exit(1);
  }

  sqlite3_stmt *stmt;
  const char *query = "SELECT COUNT(1) FROM benchmarks";
  rc = sqlite3_prepare_v2(db, query, -1, &stmt, NULL);
  if (rc != SQLITE_OK) {
    std::cerr << "Failed to execute query: " << sqlite3_errmsg(db) << std::endl;
    exit(1);
  }

  rc = sqlite3_step(stmt);
  if (rc == SQLITE_ROW) {
    no_benchs = sqlite3_column_int(stmt, 0);
    no_benchs = std::round(no_benchs * SCALER);
  } else {
    std::cerr << "Failed to fetch data: " << sqlite3_errmsg(db) << std::endl;
    exit(1);
  }
  sqlite3_finalize(stmt);

  query = "SELECT id, benchmark_usage_count, name, start_line, end_line FROM "
          "functions";
  rc = sqlite3_prepare_v2(db, query, -1, &stmt, NULL);
  if (rc != SQLITE_OK) {
    std::cerr << "Failed to execute query: " << sqlite3_errmsg(db) << std::endl;
    exit(1);
  }

  while ((rc = sqlite3_step(stmt)) == SQLITE_ROW) {
    int uid = sqlite3_column_int(stmt, 0);
    int bcount = sqlite3_column_int(stmt, 1);
    int start = sqlite3_column_int(stmt, 3);
    int end = sqlite3_column_int(stmt, 4);

    uids.push_back(uid);
    len_c.push_back(end - start + 1);

    int li = std::round(bcount * SCALER);
    std::vector<bool> Ci(no_benchs, 0);
    // FIXME: atm this is mock data, we will need to replace this with actual
    // data
    for (int i = 0; i < li; ++i) {
      Ci[i] = true;
    }
    C.push_back(Ci);
  }
  sqlite3_finalize(stmt);
  sqlite3_close(db);

  // Transpose C to get B
  n = uids.size();
  B.resize(no_benchs, std::vector<bool>(n, 0));
  for (int i = 0; i < n; ++i) {
    for (int j = 0; j < no_benchs; ++j) {
      B[j][i] = C[i][j];
    }
  }
}

GRBEnv *get_env_from_license(const std::string &file_path) {
  GRBEnv *env;
  std::ifstream file(file_path);
  if (!file) {
    // If license file does not exist, return default environment
    env = new GRBEnv();
  } else {
    env = new GRBEnv(true);
    std::string line;
    while (std::getline(file, line)) {
      if (line.find("WLSACCESSID") == 0) {
        std::string aid = line.substr(line.find('=') + 1);
        env->set(GRB_StringParam_WLSAccessID, aid);
      } else if (line.find("WLSSECRET") == 0) {
        std::string secret = line.substr(line.find('=') + 1);
        env->set(GRB_StringParam_WLSSecret, secret);
      } else if (line.find("LICENSEID") == 0) {
        int lid = std::stoi(line.substr(line.find('=') + 1));
        env->set(GRB_IntParam_LicenseID, lid);
      }
    }
    env->start();
  }
  return env;
}

int main() {
  try {
    std::cout << " |>> Extracting values from DB" << std::endl;
    double p = 0.99;
    int no_benchs, n;
    std::vector<int> uids;
    std::vector<int> len_c;
    std::vector<std::vector<bool>> C;
    std::vector<std::vector<bool>> B;

    get_function_stats_from_db(no_benchs, n, uids, len_c, C, B);

    GRBEnv *env = get_env_from_license(LICENSE_FILE);
    GRBModel model = GRBModel(*env);
    model.set(GRB_StringAttr_ModelName, MODEL_NAME);

    std::cout << " |>> Preparing optimization" << std::endl;

    // Add variables O
    std::vector<GRBVar> O(n);
    for (int i = 0; i < n; ++i) {
      O[i] = model.addVar(0.0, 1.0, len_c[i], GRB_BINARY,
                          "O_" + std::to_string(i));
    }

    // Add variables z
    std::vector<GRBVar> z(no_benchs);
    for (int i = 0; i < no_benchs; ++i) {
      z[i] = model.addVar(0.0, 1.0, 0.0, GRB_BINARY, "z_" + std::to_string(i));
    }

    // Add constraints
    for (int i = 0; i < no_benchs; ++i) {
      std::vector<int> J_i;
      for (int j = 0; j < n; ++j) {
        if (B[i][j]) {
          J_i.push_back(j);
        }
      }
      // For each j in J_i, add z[i] <= O[j]
      for (int j : J_i) {
        model.addConstr(z[i] <= O[j], "c_bench_" + std::to_string(i) +
                                          "_upper_" + std::to_string(j));
      }
      // Add constraint z[i] >= sum_j O[j] - len(J_i) + 1
      GRBLinExpr sum_Oj = 0;
      for (int j : J_i) {
        sum_Oj += O[j];
      }
      model.addConstr(z[i] >= sum_Oj - J_i.size() + 1,
                      "c_bench_" + std::to_string(i) + "_lower");
    }

    // Add constraint z.sum() >= p * no_benchs
    GRBLinExpr sum_z = 0;
    for (int i = 0; i < no_benchs; ++i) {
      sum_z += z[i];
    }
    model.addConstr(sum_z >= p * no_benchs, "c0");

    std::cout << " |>> Running optimization" << std::endl;

    // Optimize model
    model.optimize();

    if (model.get(GRB_IntAttr_Status) == GRB_OPTIMAL) {
      double objVal = model.get(GRB_DoubleAttr_ObjVal);
      // Total code length before optimization
      std::cout << "Total code length:" << std::endl;
      double total_length_before = 0.0;
      for (int i = 0; i < n; ++i) {
        // Assuming c[i] = 1 for all functions before optimization
        total_length_before += len_c[i];
      }
      std::cout << "\tbefore optimization: " << total_length_before
                << std::endl;

      // Total code length after optimization
      double total_length_after = 0.0;
      for (int i = 0; i < n; ++i) {
        double O_value = O[i].get(GRB_DoubleAttr_X);
        total_length_after += len_c[i] * O_value;
      }
      std::cout << "\tafter optimization: " << total_length_after << std::endl;

      // Achieved constraint calculation
      double lhs = 0.0;
      double sum_functions = 0.0;
      for (int i = 0; i < n; ++i) {
        double O_value = O[i].get(GRB_DoubleAttr_X);
        sum_functions += O_value;
      }
      for (int i = 0; i < no_benchs; ++i) {
        double z_value = z[i].get(GRB_DoubleAttr_X);
        lhs += z_value;
      }
      double rhs = p * no_benchs;
      std::cout << "Achieved constraint (Required Successful Benchmarks): "
                << lhs << " >= " << rhs << std::endl;

      std::cout << "No functions in use: " << sum_functions << std::endl;
      std::cout << "Objective: " << objVal << std::endl;

      std::cout << " |>> Storing result in DB" << std::endl;
      std::vector<bool> func_state(n);
      for (int i = 0; i < n; ++i) {
        func_state[i] = (O[i].get(GRB_DoubleAttr_X) > 0.5);
      }
      store_used_functions_to_db(func_state, uids, p);
    } else {
      int status = model.get(GRB_IntAttr_Status);
      std::string statusStr;
      if (status == GRB_INFEASIBLE) {
        statusStr = "INFEASIBLE";
      } else if (status == GRB_UNBOUNDED) {
        statusStr = "UNBOUNDED";
      } else if (status == GRB_INF_OR_UNBD) {
        statusStr = "INF_OR_UNBD";
      } else {
        statusStr = "Unknown case (" + std::to_string(status) + ")";
      }
      std::cout << "Could not find optimal result. Exit status: " << statusStr
                << std::endl;
    }

    delete env;
  } catch (GRBException &e) {
    std::cout << "Error code = " << e.getErrorCode() << std::endl;
    std::cout << e.getMessage() << std::endl;
  } catch (std::exception &e) {
    std::cout << "Exception during optimization: " << e.what() << std::endl;
  }
  return 0;
}
