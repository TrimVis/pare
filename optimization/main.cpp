#include "gurobi_c++.h"
#include "util.h"

#include <cassert>
#include <cmath>
#include <cstdlib>
#include <gurobi_c.h>
#include <iostream>
#include <optional>
#include <sqlite3.h>
#include <string>
#include <unistd.h>
#include <vector>

const std::string BASE_MODEL_NAME = "benchopt";

int main(int argc, char *argv[]) {
  std::string db_file = "./reports/report.sqlite";
  std::string license_file = "./optimization/gurobi.lic";
  // Scales the input size down if set
  std::optional<float> scaler = {};

  int opt;
  while ((opt = getopt(argc, argv, "l:d:s:")) != -1) {
    switch (opt) {
    case 'l':
      license_file = optarg;
      continue;
    case 'd':
      db_file = optarg;
      break;
    case 's':
      scaler = std::stof(optarg);
      break;
    case 'h':
    case '?':
    default:
      std::cout << "Help/Usage Example" << std::endl
                << argv[0]
                << " -s <SAMPLE_FACTOR> -d <DB_PATH> -l <GUROBI_LICENSE_FILE> "
                   "<P-VALUE> "
                   "[<ADD-P-VALUES>...]"
                << std::endl;
      exit(0);
    }
  }

  std::cout << " |>> Extracting values from DB" << std::endl;
  int no_benchs, n;
  std::vector<int> uids;
  std::vector<int> len_c;
  std::vector<std::vector<bool>> B;
  get_function_stats_from_db(db_file, no_benchs, n, uids, len_c, B, scaler);

  long pages = sysconf(_SC_AVPHYS_PAGES);
  long page_size = sysconf(_SC_PAGE_SIZE);

  if (pages == -1 || page_size == -1) {
    std::cerr << "Error getting memory information" << std::endl;
    return 1;
  }
  long available_mem = pages * page_size / (1024 * 1024 * 1024);
  long node_file_mem = 0.5 * available_mem;
  std::cout << " |>> Using Nodefile after 50% of memory is in use ("
            << node_file_mem << "GB)" << std::endl;

  try {
    for (int i = optind; i < argc; i++) {
      double p = std::stod(argv[i]);
      assert(p <= 1.0 && "Expected a p value of <=1.0");
      std::cout << std::endl
                << std::endl
                << " |>> Starting optimization run for p=" << p << std::endl;

      GRBEnv *env = get_env_from_license(license_file);
      GRBModel model = GRBModel(*env);
      std::string model_name = BASE_MODEL_NAME + "_p" + std::to_string(p);
      model.set(GRB_StringAttr_ModelName, model_name);
      model.set(GRB_DoubleParam_NodefileStart, node_file_mem);

      // Better logging
      model.set(GRB_IntParam_LogToConsole, 1);
      model.set(GRB_StringParam_LogFile, "gurobi.log");

      std::cout << " |>> Preparing optimization" << std::endl;

      // Add variables O (results in objective sum_j O[j] * len(c_j))
      std::vector<GRBVar> O(n);
      for (int i = 0; i < n; ++i) {
        O[i] = model.addVar(0.0, 1.0, len_c[i], GRB_BINARY,
                            "O_" + std::to_string(i));
      }

      // Add constraint that ensures z[i] = Prod for j in C_i (O[j])
      std::vector<GRBVar> z(no_benchs);
      for (int i = 0; i < no_benchs; ++i) {
        std::string var_name = "z_" + std::to_string(i);
        z[i] = model.addVar(0.0, 1.0, 1.0, GRB_BINARY, var_name);

        std::string constr_name = var_name + "_prod_";
        GRBLinExpr sum_o = 0;
        int fac = 0;
        for (int j = 0; j < n; ++j) {
          if (B[j][i]) {
            fac += 1;
            sum_o += O[j];
          }
        }

        model.addConstr(0 <= z[i], constr_name + "lower");
        model.addConstr(fac * z[i] <= sum_o, constr_name + "upper");
      }

      // Add main constraint z.sum() >= p * no_benchs
      GRBLinExpr sum_z = 0;
      for (int i = 0; i < no_benchs; ++i) {
        sum_z += z[i];
      }
      model.addConstr(sum_z >= p * no_benchs, "c0");

      // // Write out the initial model
      // std::cout << " |>> Storing initial model" << std::endl;
      // model.write("initial_model_" + model_name + ".lp");

      // 10 hour max runtime limit
      const double max_run_time = 60.0 * 60.0 * 10.0;

      // Parameters for iterative solving:
      double time_limit = 1800.0; // 30 minutes per iteration
      model.set(GRB_DoubleParam_TimeLimit, time_limit);

      double run_time = 0.0;
      while (run_time <= max_run_time) {
        std::cout << " |>> Running optimization step" << std::endl;
        model.optimize();

        int status = model.get(GRB_IntAttr_Status);

        if (status == GRB_TIME_LIMIT) {
          std::cout << " |>> Time limit reached, saving checkpoint."
                    << std::endl;

          // If a feasible solution has been found, write it out
          double objVal = model.get(GRB_DoubleAttr_ObjVal);
          if (objVal < GRB_INFINITY) {
            model.write("checkpoint_solution_" + model_name + ".sol");
            std::cout << " |>> Feasible solution saved to checkpoint_solution_"
                      << model_name << ".sol" << std::endl;
          }

          // // Write out the model as well
          // model.write("checkpoint_model_" + model_name + ".lp");
          // std::cout << " |>> Model written to checkpoint_model_" <<
          // model_name
          //           << ".lp" << std::endl;

          run_time += time_limit;
          // Extend time limit by 15 minutes
          time_limit += 900.0;
          model.set(GRB_DoubleParam_TimeLimit, time_limit);
        } else {
          if (status == GRB_OPTIMAL) {
            std::cout << " |>> Optimal solution found." << std::endl;
          } else if (status == GRB_INFEASIBLE) {
            std::cout << " |>> Model is infeasible." << std::endl;
          } else if (status == GRB_UNBOUNDED) {
            std::cout << " |>> Model is unbounded." << std::endl;
          } else if (status == GRB_INF_OR_UNBD) {
            std::cout << " |>> Model is infeasible or unbounded." << std::endl;
          } else {
            std::cout << " |>> Unexpected status " << status << ", stopping."
                      << std::endl;
          }
          break;
        }
      }

      std::cout << std::endl << " |>> Optimization concluded" << std::endl;
      double objVal = model.get(GRB_DoubleAttr_ObjVal);
      if (objVal < GRB_INFINITY) {
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
        std::cout << "\tafter optimization: " << total_length_after
                  << std::endl;

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

        std::vector<bool> func_state(n);
        for (int i = 0; i < n; ++i) {
          func_state[i] = (O[i].get(GRB_DoubleAttr_X) > 0.5);
        }
        store_used_functions_to_db(db_file, func_state, uids, p);
        std::cout << " |>> Feasible solution saved to DB" << std::endl;

        model.write("solution_" + model_name + ".sol");
        std::cout << " |>> Feasible solution saved to solution_" << model_name
                  << ".sol" << std::endl;
      }

      // Write out the model as well
      model.write("model_" + model_name + ".lp");
      std::cout << " |>> Model written to model_" << model_name << ".lp"
                << std::endl;

      model.reset();
      delete env;
    }
  } catch (GRBException &e) {
    std::cout << "Error code = " << e.getErrorCode() << std::endl;
    std::cout << e.getMessage() << std::endl;
  } catch (std::exception &e) {
    std::cout << "Exception during optimization: " << e.what() << std::endl;
  } catch (...) {
    std::cerr << "Unknown exception caught." << std::endl;
  }
  return 0;
}
