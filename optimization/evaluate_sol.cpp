#include <cassert>
#include <cmath>
#include <cstdint>
#include <cstdlib>
#include <fstream>
#include <iostream>
#include <map>
#include <sqlite3.h>
#include <sstream>
#include <stdexcept>
#include <string>
#include <vector>

const std::string DB_FILE = "./reports/report.sqlite";

void get_function_stats_from_db(int &no_benchs, int &n, std::vector<int> &uids,
                                std::vector<int> &len_c,
                                std::vector<std::vector<bool>> &B) {
  sqlite3 *db;
  int rc = sqlite3_open(DB_FILE.c_str(), &db);
  if (rc) {
    std::cerr << "Can't open database: " << sqlite3_errmsg(db) << std::endl;
    exit(1);
  }

  sqlite3_stmt *stmt;
  const char *query = "SELECT MAX(benchmark_usage_count) FROM functions";
  rc = sqlite3_prepare_v2(db, query, -1, &stmt, NULL);
  if (rc != SQLITE_OK) {
    std::cerr << "Failed to execute query: " << sqlite3_errmsg(db) << std::endl;
    exit(1);
  }

  rc = sqlite3_step(stmt);
  if (rc == SQLITE_ROW) {
    no_benchs = sqlite3_column_int(stmt, 0);
    no_benchs = std::round(no_benchs);
  } else {
    std::cerr << "Failed to fetch data: " << sqlite3_errmsg(db) << std::endl;
    exit(1);
  }
  sqlite3_finalize(stmt);

  query = "SELECT id, benchmark_usage_count, start_line, end_line FROM "
          "functions";
  rc = sqlite3_prepare_v2(db, query, -1, &stmt, NULL);
  if (rc != SQLITE_OK) {
    std::cerr << "Failed to execute query: " << sqlite3_errmsg(db) << std::endl;
    exit(1);
  }

  while ((rc = sqlite3_step(stmt)) == SQLITE_ROW) {
    int uid = sqlite3_column_int(stmt, 0);
    int bcount = sqlite3_column_int(stmt, 1);
    int start = sqlite3_column_int(stmt, 2);
    int end = sqlite3_column_int(stmt, 3);

    uids.push_back(uid);
    len_c.push_back(end - start + 1);
  }
  sqlite3_finalize(stmt);

  n = uids.size();

  query = "SELECT function_id, data FROM function_bitvecs";
  if (sqlite3_prepare_v2(db, query, -1, &stmt, nullptr) != SQLITE_OK) {
    std::cerr << "Error preparing SQL statement\n";
    sqlite3_close(db);
    exit(1);
  }

  B.reserve(n);
  // Process each row
  while (sqlite3_step(stmt) == SQLITE_ROW) {
    // Read source_id and function_id as integers
    int function_id = sqlite3_column_int(stmt, 0);

    // Read the BLOB data
    const void *blob_data = sqlite3_column_blob(stmt, 1);
    int blob_size = sqlite3_column_bytes(stmt, 1);

    std::vector<bool> Bi(no_benchs, 0);
    Bi.reserve(blob_size * 8);
    const uint8_t *data = static_cast<const uint8_t *>(blob_data);

    for (int i = 0; i < blob_size; ++i) {
      for (int bit = 0; bit < 8; ++bit) {
        Bi[i] = (data[i] >> bit) & 1;
      }
    }
    B.push_back(Bi);
  }
  sqlite3_finalize(stmt);

  sqlite3_close(db);
}

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
  std::cout << " |>> Extracting information from DB" << std::endl;
  int no_benchs, n;
  std::vector<int> uids;
  std::vector<int> len_c;
  std::vector<std::vector<bool>> B;
  get_function_stats_from_db(no_benchs, n, uids, len_c, B);

  for (int i = 1; i < argc; i++) {
    auto filename = std::string(argv[i]);
    std::cout << " |>> Evaluating solution file '" << filename << "'"
              << std::endl;
    auto opt_solution = evaluate_solution_file(filename);

    auto O = opt_solution["O"];
    auto z = opt_solution["z"];

    // Total code length before optimization
    std::cout << "Total code length:" << std::endl;
    double total_length_before = 0.0;
    for (int i = 0; i < n; ++i) {
      // Assuming c[i] = 1 for all functions before optimization
      total_length_before += len_c[i];
    }
    std::cout << "\tbefore optimization: " << total_length_before << std::endl;

    // Total code length after optimization
    double total_length_after = 0.0;
    for (int i = 0; i < n; ++i) {
      total_length_after += len_c[i] * O[i];
    }
    std::cout << "\tafter optimization: " << total_length_after << std::endl;

    // Achieved constraint calculation
    double lhs = 0.0;
    double sum_functions = 0.0;
    for (int i = 0; i < n; ++i) {
      sum_functions += O[i];
    }
    for (int i = 0; i < no_benchs; ++i) {
      lhs += z[i];
    }
    std::cout << "No. Required Successful Benchmarks: " << lhs << std::endl;
    std::cout << "No functions in use: " << sum_functions << std::endl;
  }

  return 0;
}
