#include "util.h"
#include <bits/getopt_core.h>
#include <cassert>
#include <cmath>
#include <cstdlib>
#include <fstream>
#include <iostream>
#include <map>
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
        arrays[arrName].resize(idx + 1, 0.0);
      }
      arrays[arrName][idx] = value;
    } else {
      // No underscore means it's a standalone variable name
      arrays[varName] = {value};
    }
  }

  return arrays;
}

int main(int argc, char *argv[]) {
  std::string db_file = "./reports/report.sqlite";

  int opt;
  while ((opt = getopt(argc, argv, "d:")) != -1) {
    switch (opt) {
    case 'd':
      db_file = optarg;
      break;
    case 'h':
    case '?':
    default:
      std::cout << "Help/Usage Example\n"
                << argv[0]
                << " -d <DB_PATH> <SOL-FILE> "
                   "[<ADD-SOL-FILES>...]"
                << std::endl;
      exit(0);
    }
  }

  std::cout << " |>> Extracting information from DB" << std::endl;
  std::vector<int> benches;
  std::vector<int> uids;
  std::vector<int> len_c;
  std::vector<std::vector<bool>> B;
  get_function_stats_from_db(db_file, benches, uids, len_c, B, {});

  for (int i = optind; i < argc; i++) {
    auto filename = std::string(argv[i]);
    std::cout << " |>> Evaluating solution file '" << filename << "'"
              << std::endl;
    auto opt_solution = evaluate_solution_file(filename);

    auto func_used = opt_solution["func_used"];
    auto bench_used = opt_solution["bench_used"];

    // Total code length before optimization
    std::cout << "Total code length:" << std::endl;
    double total_length_before = 0.0;
    for (int i = 0; i < uids.size(); ++i) {
      // Assuming c[i] = 1 for all functions before optimization
      total_length_before += len_c[i];
    }
    std::cout << "\tbefore optimization: " << total_length_before << std::endl;

    // Total code length after optimization
    double total_length_after = 0.0;
    for (int i = 0; i < uids.size(); ++i) {
      total_length_after += len_c[i] * func_used[i];
    }
    std::cout << "\tafter optimization: " << total_length_after << std::endl;

    // Achieved constraint calculation
    double lhs = 0.0;
    double sum_functions = 0.0;
    for (int i = 0; i < uids.size(); ++i) {
      sum_functions += func_used[i];
    }
    for (int i = 0; i < benches.size(); ++i) {
      lhs += bench_used[i];
    }
    std::cout << "No. Required Successful Benchmarks: " << lhs << std::endl;
    std::cout << "No functions in use: " << sum_functions << std::endl;
  }

  return 0;
}
