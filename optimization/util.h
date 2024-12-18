#include "gurobi_c++.h"
#include <optional>

void store_used_functions_to_db(std::string db_file,
                                std::vector<bool> &func_state,
                                std::vector<int> &func_ids, float p);

void get_function_stats_from_db(std::string db_file, int &no_benchs, int &n,
                                std::vector<int> &uids, std::vector<int> &len_c,
                                std::vector<std::vector<bool>> &B,
                                std::optional<double> scaler);

GRBEnv *get_env_from_license(const std::string &file_path);
