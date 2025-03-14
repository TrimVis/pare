#include "util.h"
#include <bits/getopt_core.h>
#include <cassert>
#include <cmath>
#include <cstdlib>
#include <fstream>
#include <iostream>
#include <map>
#include <optional>
#include <sqlite3.h>
#include <sstream>
#include <stdexcept>
#include <string>
#include <vector>

std::map<std::string, std::vector<double>>
evaluate_solution_file(std::string &filename) {
  // This map will hold arrays keyed by their name, with values stored in a
  // vector. For variables that do not have an underscore (no explicit index),
  // we can store them as a single-element vector.
  std::map<std::string, std::vector<double>> arrays;

  std::ifstream inFile(filename);
  if (!inFile.is_open()) {
    std::cerr << "Error: Could not open file " << filename << "\n";
    return arrays;
  }

  std::string line;
  while (std::getline(inFile, line)) {
    if (line.empty())
      continue; // Skip empty lines

    std::istringstream iss(line);
    std::string varName;
    double value;

    if (!(iss >> varName >> value)) {
      std::cerr << "Warning: Could not parse line: " << line << "\n";
      continue;
    }

    // Check if variable name has the form arrName_index
    // Find the last underscore to separate array name and index.
    std::size_t underscorePos = varName.rfind('_');
    if (underscorePos != std::string::npos) {
      // Extract array name and index
      std::string arrName = varName.substr(0, underscorePos);
      std::string indexStr = varName.substr(underscorePos + 1);

      int idx = 0;
      try {
        idx = std::stoi(indexStr);
      } catch (const std::invalid_argument &e) {
        std::cerr << "Error: Invalid index in variable name \"" << varName
                  << "\".\n";
        continue;
      }

      if (static_cast<size_t>(idx) >= arrays[arrName].size()) {
        arrays[arrName].resize(idx + 10);
      }
      arrays[arrName][idx] = value;
    } else {
      // No underscore means it's a standalone variable name
      arrays[varName] = {value};
    }
  }

  return arrays;
}

std::vector<bool> get_evaluation_data(std::string &db_file,
                                      std::string &table_name) {
  std::vector<bool> eval_result;
  sqlite3 *db;
  int rc = sqlite3_open(db_file.c_str(), &db);
  if (rc) {
    std::cerr << "Can't open database: " << sqlite3_errmsg(db) << std::endl;
    exit(1);
  }

  sqlite3_stmt *stmt;
  std::string query =
      "select r.bench_id, (e.stdout not like '%Unsupported%') as \"supported\""
      " from result_benchmarks as r"
      " join \"" +
      table_name +
      "\" as e ON e.bench_id = r.bench_id"
      " order by r.bench_id;";
  std::cout << query << std::endl;
  rc = sqlite3_prepare_v2(db, query.c_str(), -1, &stmt, NULL);
  if (rc != SQLITE_OK) {
    std::cerr << "Failed to execute query: " << sqlite3_errmsg(db) << std::endl;
    sqlite3_close(db);
    exit(1);
  }

  eval_result.push_back(false);
  for (int i = 1; (rc = sqlite3_step(stmt)) == SQLITE_ROW; i++) {
    int id = sqlite3_column_int(stmt, 0);
    // assert(id == i && "Out of order bench id!");
    eval_result.push_back(!!sqlite3_column_double(stmt, 1));
  }
  sqlite3_finalize(stmt);

  sqlite3_close(db);

  return eval_result;
}

int main(int argc, char *argv[]) {
  std::string db_file = "./reports/report.sqlite";
  std::optional<std::string> eval_table = {};
  bool exec_cvc5 = false;

  int opt;
  while ((opt = getopt(argc, argv, "d:e:c")) != -1) {
    switch (opt) {
    case 'd':
      db_file = optarg;
      break;
    case 'e':
      eval_table = optarg;
      break;
    case 'c':
      exec_cvc5 = true;
      break;
    case 'h':
    case '?':
    default:
      std::cout << "Help/Usage Example\n"
                << argv[0]
                << " -d <DB_PATH> -e <EVAL_TABLE_NAME> [-c] <SOL-FILE> "
                   "[<ADD-SOL-FILES>...]"
                << std::endl;
      exit(0);
    }
  }

  std::cout << " |>> Extracting information from DB" << std::endl;
  std::vector<int> bench_ids;
  std::vector<int> func_ids;
  std::vector<int> func_lens;
  std::vector<std::vector<bool>> func_usages;
  std::optional<std::vector<bool>> eval_data = {};
  get_function_stats_from_db(db_file, bench_ids, func_ids, func_lens,
                             func_usages, {});
  if (eval_table.has_value()) {
    std::cout << " |>> Extracting evaluation data from DB" << std::endl;
    eval_data = get_evaluation_data(db_file, eval_table.value());
  }

  for (int i = optind; i < argc; i++) {
    auto filename = std::string(argv[i]);
    std::cout << " |>> Evaluating solution file '" << filename << "'"
              << std::endl;
    auto opt_solution = evaluate_solution_file(filename);

    auto func_used = opt_solution["func"];
    auto bench_used = opt_solution["bench"];

    // Total code length before optimization
    std::cout << "Total code length:" << std::endl;
    double total_length_before = 0.0;
    for (int i = 0; i < func_ids.size(); ++i) {
      // Assuming c[i] = 1 for all functions before optimization
      total_length_before += func_lens[i];
    }
    std::cout << "\tbefore optimization: " << total_length_before << std::endl;

    // Total code length after optimization
    double total_length_after = 0.0;
    for (int i = 0; i < func_ids.size(); ++i) {
      total_length_after += func_lens[i] * func_used[func_ids[i]];
    }
    std::cout << "\tafter optimization: " << total_length_after << std::endl;

    // Achieved constraint calculation
    double lhs = 0.0;
    double sum_functions = 0.0;
    for (int i = 0; i < func_ids.size(); ++i) {
      sum_functions += func_used[func_ids[i]];
    }
    for (int i = 0; i < bench_ids.size(); ++i) {
      lhs += bench_used[bench_ids[i]];
    }
    std::cout << "No functions in use: " << sum_functions << std::endl;
    std::cout << "No working benchmarks: " << lhs << std::endl;

    std::cout << std::endl
              << "Overview of working benchmarks per theory:" << std::endl;

    std::vector<std::string> bench_names = get_bench_stats_from_db(db_file);
    std::map<std::string, std::tuple<int, int>> rel_theory_working;
    for (int j = 0; j < bench_ids.size(); j++) {
      int bench_id = bench_ids[j];
      std::string path = bench_names[j];

      // Extract the theory name
      std::string marker = "/non-incremental/";
      size_t pos = path.find(marker);
      if (pos == std::string::npos) {
        assert("Found invalid path string");
        continue;
      }
      pos += marker.size();
      size_t endPos = path.find('/', pos);
      std::string theory = (endPos != std::string::npos)
                               ? path.substr(pos, endPos - pos)
                               : path.substr(pos);

      // Update the map entry
      int working = bench_used[bench_id];
      int total = 1;
      if (auto elem = rel_theory_working.find(theory);
          elem != rel_theory_working.end()) {
        working += std::get<0>(elem->second);
        total += std::get<1>(elem->second);
      }

      rel_theory_working[theory] = {working, total};
    }

    int count = 1;
    for (auto theory_elem : rel_theory_working) {
      std::string theory_name = theory_elem.first + ":";
      theory_name.resize(15, ' ');
      int abs_notworking =
          std::get<1>(theory_elem.second) - std::get<0>(theory_elem.second);
      int percentage = 100.0 * (double)std::get<0>(theory_elem.second) /
                       (double)std::get<1>(theory_elem.second);
      std::cout << theory_name << percentage << "% (-" << abs_notworking
                << ")\t";
      if (count % 1 == 0) {
        std::cout << std::endl;
      }
      count++;
    }
    std::cout << std::endl;

    if (eval_data.has_value()) {
      int overall_success = 0;
      int overall_wrong = 0;
      for (int j = 0; j < bench_ids.size(); j++) {
        int id = bench_ids[j];
        if (eval_data.value()[id] != bench_used[id]) {
          // Actually run this benchmark just to be sure

          std::cout << "Sanity check, executing benchmark..." << std::endl;
          std::string command =
              "../cvc5-repo/build/bin/cvc5 --timeout 5000 " + bench_names[j];
          int result = std::system(command.c_str());

          if (result != 0 && result != 1) {
            overall_wrong += 1;
            std::cout << "Expected benchmark (id:  " << id << ") "
                      << bench_names[j] << " to "
                      << (bench_used[id] ? "terminate successfully" : "fail")
                      << ", but it did not!" << std::endl;

            std::cout << "Expected: " << bench_used[id]
                      << "; Evaluation Result: " << eval_data.value()[id]
                      << "; Execution Result: " << (result == 0 || result == 1)
                      << std::endl;
          } else {
            overall_success += 1;
          }
        }
      }
      std::cout << "Reported errorneous benchmarks:"
                << "\n\t without Errors: \t " << overall_success
                << "\n\t with Errors: \t " << overall_wrong << std::endl;
    }

    // Some line breaks so we have clearer borders
    if (i < argc - 1) {
      std::string spacer = "";
      spacer.resize(30, '-');
      std::cout << std::endl << spacer << std::endl << std::endl;
    }
  }

  return 0;
}
