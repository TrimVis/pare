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
const int MAX_RUNS = 2;

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

  for (int i = optind; i < argc; i++) {
    double p = std::stod(argv[i]);
    if (p > 1.0) {
      std::cout << "All p values have to be <= 1.0" << std::endl;
      exit(1);
    }
  }

  std::cout << " |>> Extracting values from DB" << std::endl;
  std::vector<int> bench_ids;
  std::vector<int> func_ids;
  std::vector<int> func_lens;
  std::vector<std::vector<bool>> func_usages;
  get_function_stats_from_db(db_file, bench_ids, func_ids, func_lens,
                             func_usages, scaler);

  // long pages = sysconf(_SC_AVPHYS_PAGES);
  // long page_size = sysconf(_SC_PAGE_SIZE);

  // if (pages == -1 || page_size == -1) {
  //   std::cerr << "Error getting memory information" << std::endl;
  //   return 1;
  // }
  // long available_mem = pages * page_size / (1024 * 1024 * 1024);
  // long node_file_mem = 0.5 * available_mem;
  // std::cout << " |>> Using Nodefile after 50% of memory is in use ("
  //           << node_file_mem << "GB)" << std::endl;

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
      // model.set(GRB_DoubleParam_NodefileStart, node_file_mem);

      // Better logging
      model.set(GRB_IntParam_LogToConsole, 1);
      model.set(GRB_StringParam_LogFile, "gurobi.log");

      // // More threads!!
      // model.set(GRB_IntParam_Threads, 128);
      // // Less threads!!
      // model.set(GRB_IntParam_Threads, 4);

      // Disable presolve
      model.set(GRB_IntParam_Presolve, 0);
      // // Make presolve conservative
      // model.set(GRB_IntParam_Presolve, 1);
      // // Limit the number of presolve passes
      // model.set(GRB_IntParam_PrePasses, 4);

      // // Focus on exploring more different bounds
      // model.set(GRB_IntParam_MIPFocus, 1);
      // In case bounds are moving slowly/not at all
      // model.set(GRB_IntParam_MIPFocus, 3);

      std::cout << " |>> Preparing optimization" << std::endl;

      std::vector<GRBVar> func_vars(func_ids.size());
      std::vector<GRBVar> bench_vars(bench_ids.size());

      // Add function indicator variables
      for (int i = 0; i < func_ids.size(); ++i) {
        func_vars[i] = model.addVar(0.0, 1.0, func_lens[i], GRB_BINARY,
                                    "func_" + std::to_string(func_ids[i]));
      }

      // Add benchmark indicator variables
      for (int i = 0; i < bench_ids.size(); ++i) {
        bench_vars[i] = model.addVar(0.0, 1.0, 0.0, GRB_BINARY,
                                     "bench_" + std::to_string(bench_ids[i]));
      }

      // Add constraint that ensures bench_used[i] = Prod for j in C_i (func[j])
      for (int b_ind = 0; b_ind < bench_ids.size(); ++b_ind) {
        std::string constr_name =
            "bench_" + std::to_string(bench_ids[b_ind]) + "_prod";

        GRBLinExpr sum_o = 0;
        int fac = 0;
        for (int f_ind = 0; f_ind < func_ids.size(); ++f_ind) {
          if (func_usages[f_ind][b_ind]) {
            fac += 1;
            sum_o += func_vars[f_ind];
          }
        }

        model.addGenConstrIndicator(bench_vars[b_ind], 1, sum_o, GRB_EQUAL, fac,
                                    constr_name);

        // model.addConstr(bench_vars[b_ind] >= sum_o - fac + 1,
        //                 constr_name + "_lower");
        // model.addConstr(fac * bench_vars[b_ind] <= sum_o,
        //                 constr_name + "_upper");
      }

      // Add main constraint bench_used.sum() >= p * no_benchs
      GRBLinExpr constraint_lhs = 0.0;
      for (int i = 0; i < bench_ids.size(); ++i) {
        constraint_lhs += bench_vars[i];
      }
      double constraint_rhs = p * bench_ids.size();
      model.addConstr(constraint_lhs >= constraint_rhs, "main");

      // // Write out the initial model
      // std::cout << " |>> Storing initial model" << std::endl;
      // model.write("initial_model_" + model_name + ".lp");

      // TUNING
      // // Parameters for tuning:
      // double tune_time_limit = 1.0 * 3600.0;
      // model.set(GRB_DoubleParam_TuneTimeLimit, tune_time_limit);

      // // Tune the initial model
      // std::cout << " |>> Tuning model" << std::endl;
      // model.tune();

      // // Apply the tuned parameters
      // int resultcount = model.get(GRB_IntAttr_TuneResultCount);
      // model.getTuneResult(resultcount - 1);

      // Parameters for iterative solving:
      double time_limit = 10.0 * 3600.0; // 10 hours per iteration
      model.set(GRB_DoubleParam_TimeLimit, time_limit);

      for (int runs = 0; runs < MAX_RUNS; runs++) {
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
      int status = model.get(GRB_IntAttr_Status);
      if (status != GRB_INFEASIBLE || status != GRB_INF_OR_UNBD) {

        double objVal = model.get(GRB_DoubleAttr_ObjVal);
        if (objVal < GRB_INFINITY) {
          double total_length_before = 0.0;
          double total_length_after = 0.0;
          double sum_functions = 0.0;
          for (int i = 0; i < func_vars.size(); ++i) {
            double func_used = func_vars[i].get(GRB_DoubleAttr_X);
            total_length_before += func_lens[i];
            total_length_after += func_lens[i] * func_used;
            sum_functions += func_used;
          }

          // Total code length comparison
          std::cout << "Total code length:"
                    << "\n\tbefore optimization: " << total_length_before
                    << "\n\tafter optimization: " << total_length_after
                    << std::endl;

          // Achieved constraint
          std::cout << "Constraint (optimized >= required bench count): \n\t"
                    << constraint_lhs.getValue() << " >= " << constraint_rhs
                    << std::endl;

          std::cout << "Total number of functions in use: \t" << sum_functions
                    << std::endl;
          std::cout << "Objective: \t" << objVal << std::endl;

          std::vector<bool> func_state(func_ids.size());
          for (int i = 0; i < func_ids.size(); ++i) {
            func_state[i] = (func_vars[i].get(GRB_DoubleAttr_X) > 0.5);
          }

          // FIXME: Also store the benches which should be running
          store_used_functions_to_db(db_file, func_state, func_ids, p);
          std::cout << " |>> Feasible solution saved to DB" << std::endl;

          model.write("solution_" + model_name + ".sol");
          std::cout << " |>> Feasible solution saved to solution_" << model_name
                    << ".sol" << std::endl;
        }
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
