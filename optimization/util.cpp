#ifndef __UTIL_H
#define __UTIL_H

#include "util.h"
#include "gurobi_c++.h"
#include <algorithm> // For std::replace
#include <cassert>
#include <cmath>
#include <cstdint>
#include <cstdlib>
#include <fstream>
#include <iomanip> // For std::setprecision
#include <iostream>
#include <optional>
#include <sqlite3.h>
#include <sstream>
#include <string>
#include <unistd.h>
#include <vector>

void store_used_functions_to_db(std::string db_file,
                                std::vector<bool> &func_state,
                                std::vector<int> &func_ids, float p) {
  sqlite3 *db;
  int rc = sqlite3_open(db_file.c_str(), &db);
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

  // Drop any existing tables
  std::ostringstream drop_table_stream;
  drop_table_stream << "DROP TABLE IF EXISTS " << table_name << ";";
  std::string drop_table_query = drop_table_stream.str();

  rc = sqlite3_exec(db, drop_table_query.c_str(), nullptr, nullptr, nullptr);
  if (rc != SQLITE_OK) {
    std::cerr << "Failed to create table: " << sqlite3_errmsg(db) << std::endl;
    sqlite3_close(db);
    exit(1);
  }

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

void get_function_stats_from_db(std::string db_file, int &no_benchs, int &n,
                                std::vector<int> &uids, std::vector<int> &len_c,
                                std::vector<std::vector<bool>> &B,
                                std::optional<double> scaler) {
  sqlite3 *db;
  int rc = sqlite3_open(db_file.c_str(), &db);
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
    if (scaler.has_value()) {
      no_benchs = std::round(no_benchs * scaler.value());
    }
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

#endif
