#!/usr/bin/env python

import time
import os
import sys

import fire
from loguru import logger 
import enlighten

import orjson
import csv
import sqlite3
import pandas as pd

import tree_sitter_cpp as tscpp
from tree_sitter import Language, Parser
from typing import Literal

from plotnine import ggplot, aes, geom_point, theme_minimal, labs, theme, element_blank, facet_wrap, geom_hline, annotate, scale_color_manual, scale_y_log10, geom_line, facet_wrap



pmanager = enlighten.get_manager(min_delta=0.5)

CPP_LANGUAGE = Language(tscpp.language())
parser = Parser(CPP_LANGUAGE)

def _read_json(file: str):
    with open(file, "rb") as f:
        data = f.read()
    return orjson.loads(data)


_prev_file_path = None
_prev_tree = None
_prev_cpp_code = None
_prev_results = {}

def _lookup_function_name(src_code: str, file_path: str, line_no: int) -> str:
    if not src_code:
        return (None, (0,0))

    global _prev_file_path, _prev_cpp_code, _prev_tree, _prev_results
    result_key = (file_path, line_no)
    if result_key in _prev_results:
        return _prev_results[result_key]
    
    full_path = os.path.join(src_code, file_path)

    # Try to extract function name using treesitter
    if file_path == _prev_file_path:
        cpp_code = _prev_cpp_code
        tree = _prev_tree
    else:
        try:
            with open(full_path, 'rb') as f:
                cpp_code = f.read()
                parser.reset()
                tree = parser.parse(cpp_code, encoding="utf8")
                _prev_file_path = file_path
                _prev_cpp_code = cpp_code
                _prev_tree = tree
        except FileNotFoundError:
            return (None, (0,0))
    def find_function(node):
        if node.start_point[0] + 1 == line_no and node.type == 'function_definition':
            frange = (node.start_point[0] + 1, node.end_point[0] + 1)
            for child in node.children:
                if child.type == 'function_declarator':
                    for cchild in child.children:
                        if cchild.type == 'identifier':
                            return (cpp_code[cchild.start_byte:cchild.end_byte].decode("utf-8"), frange)
                    return (cpp_code[child.start_byte:child.end_byte].decode("utf-8"), frange)
        
        for child in node.children:
            if res := find_function(child):
                return res

        return None

    res = find_function(tree.root_node)
    if res:
        _prev_results[result_key] = res
        return res

    # TODO pjordan: I don't think this fallback is really needed
    # Figure out what types the nodes are for which this fails

    full_path = os.path.join(src_code, file_path)
    with open(full_path, 'r') as f:
        for i, line in enumerate(f, start=1):
            if i == line_no:
                return (line.strip().rstrip("{"), (line_no,-1))

    # Not enough line numbers in this file
    return ("-", (-1,-1))

def _clean_path(path: str) -> str:
    # Find the first occurrences of "src", "build", and "include"
    src_index = path.find("src")
    build_index = path.find("build")
    include_index = path.find("include")

    # List of all valid indices (non-negative ones)
    indices = [i for i in [src_index, build_index, include_index] if i != -1]

    # Determine the earliest occurrence of "src", "build", or "include"
    if indices:
        earliest_index = min(indices)
        return path[earliest_index:]
    
    # If none of "src", "build", or "include" are found, return the original path
    return path

def _shorten_path_cblk(group_depth):
    def shorten_path(file):
        cs = file.split(os.sep)
        if cs[-1].endswith((".cpp",".c",".h",".hpp")):
            cs = cs [:-1]

        if len(cs) > group_depth:
            return os.sep.join(cs[:group_depth])

        return os.sep.join(cs)

    return shorten_path

SortT = Literal["ASC"] | Literal["DESC"] | bool
KindT = Literal["line"] | Literal["func"] | Literal["fline"]

class JsonAnalyzer:
    def get_data(kind: KindT, input: str, src_code: str, sort: SortT = "DESC"):
        if kind == "line":
            return JsonAnalyzer.get_line_data(input, src_code, sort)
        elif kind == "func":
            return JsonAnalyzer.get_func_data(input, src_code, sort)
        elif kind == "fline":
            return JsonAnalyzer.get_fline_data(input, src_code, sort)

    def get_line_data(input: str, src_code: str, sort: SortT = "DESC"):
        logger.info(f"Reading line usage data from {input}")
        d = _read_json(input)
        src_code = src_code.rstrip("/").rstrip("src")

        res_header = ["uid", "execution_count", "file_path", "line_no"]
        res_sqlite_types = ["TEXT PRIMARY KEY", "INTEGER", "TEXT", "INTEGER"]
        res_data = [  ]

        pbar = pmanager.counter(total=len(d["sources"].items()), desc='line', unit='source files')
        for (path, value) in d["sources"].items():
            # there is always a object with only the empty key
            value = value[""]
            lines = value["lines"]
            cleaned_path = _clean_path(path)
            for (line_no, exec_count) in lines.items():
                uid = f"{cleaned_path}:{line_no}"
                res_data.append(
                    [uid, exec_count, cleaned_path, line_no]
                )
            pbar.update()
        pbar.close()

        # Sort the result data, if wanted
        if sort:
            reverse_sort = sort.lower() if isinstance(sort, str) else False
            logger.info(f"Sorting usage data by execution count in {'descending' if reverse_sort else 'ascending'} order")
            key_fn = lambda i: i[1]
            res_data.sort(key=key_fn, reverse=reverse_sort)

        return (res_data, res_header, res_sqlite_types)



    def get_func_data(input: str, src_code: str, sort: SortT = False):
        logger.info(f"Reading function usage data from {input}")
        d = _read_json(input)
        src_code = src_code.rstrip("/").rstrip("src")

        res_header = ["uid", "execution_count", "file", "func_name", "start_line", "end_line"]
        res_sqlite_types = ["TEXT PRIMARY KEY", "INTEGER", "TEXT", "TEXT", "INTEGER", "INTEGER"]
        res_data = [  ]

        pbar = pmanager.counter(total=len(d["sources"].items()), desc='func', unit='source files')
        for (path, value) in d["sources"].items():
            # there is always a object with only the empty key
            value = value[""]
            functions = value["functions"]
            branches = value["branches"]
            lines = value["lines"]
            cleaned_path = _clean_path(path)

            for (func_id, func_value) in functions.items():
                exec_count = func_value["execution_count"]

                (func_name, frange) = _lookup_function_name(src_code, cleaned_path, func_value["start_line"])
                uid = f"{cleaned_path}:{func_id}"
                res_data.append(
                    [uid, exec_count, cleaned_path, func_name, frange[0], frange[1]]
                )

            pbar.update()
        pbar.close()

        # Sort the result data, if wanted
        if sort:
            reverse_sort = sort.lower() if isinstance(sort, str) else False
            logger.info(f"Sorting usage data by function execution count in {'descending' if reverse_sort else 'ascending'} order")
            key_fn = lambda i: i[1]
            res_data.sort(key=key_fn, reverse=reverse_sort)

        return (res_data, res_header, res_sqlite_types)




    def get_fline_data(input: str, src_code: str, sort: SortT = False):
        logger.info(f"Reading function-line usage data from {input}")
        d = _read_json(input)
        src_code = src_code.rstrip("/").rstrip("src")

        res_header = ["uid", "execution_count", "file", "func_name", "line_no", "func_no_lines"]
        res_sqlite_types = ["TEXT PRIMARY KEY", "INTEGER", "TEXT", "TEXT", "INTEGER", "INTEGER"]
        res_data = [  ]
        
        pbar = pmanager.counter(total=len(d["sources"].items()), desc='fline', unit='source files')
        for (path, value) in d["sources"].items():
            # there is always a object with only the empty key
            value = value[""]
            functions = value["functions"]
            branches = value["branches"]
            lines = value["lines"]
            cleaned_path = _clean_path(path)

            func_line_map = []
            for (func_id, func_value) in functions.items():
                (func_name, frange) = _lookup_function_name(src_code, cleaned_path, func_value["start_line"])
                func_line_map.append(
                    (cleaned_path, func_id, func_name, frange[0], frange[1])
                )

            curr_func_i = 0
            f_data = []
            for (line_no, exec_count) in lines.items():

                # Check if line falls into next function
                if curr_func_i < (len(func_line_map) - 1) and int(line_no) >= int(func_line_map[curr_func_i][3]):
                    curr_func_i += 1

                (func_path, func_id, func_name, func_start, func_end) = func_line_map[curr_func_i]
                uid = f"{func_path}:{func_id}:l{line_no}"
                res_data.append(
                    [uid, exec_count, func_path, func_name, line_no, func_end - func_start if func_end != -1 else -1]
                )

            pbar.update()
        pbar.close()

        # Sort the result data, if wanted
        if sort:
            reverse_sort = sort.lower() if isinstance(sort, str) else False
            logger.info(f"Sorting usage data by execution count in {'descending' if reverse_sort else 'ascending'} order")
            key_fn = lambda i: i[1]
            res_data.sort(key=key_fn, reverse=reverse_sort)

        return (res_data, res_header, res_sqlite_types)


class Csv:
    def fline_usage(self, input: str="./coverage.json", output: str="./out_function_lines.csv", src_code: str=None, sort: SortT=False):
        (res_data, res_header, _) = JsonAnalyzer.get_fline_data(input, src_code, sort)

        logger.info(f"Creating csv file at {output}")
        # Insert header
        res_data.insert(0, res_header)
        with open(output, 'w', newline='') as f:
            writer = csv.writer(f)
            writer.writerows(res_data)

    def func_usage(self, input: str="./coverage.json", output: str="./out_functions.csv", src_code: str=None, sort: SortT="asc"):
        (res_data, res_header, _) = JsonAnalyzer.get_func_data(input, src_code, sort)

        logger.info(f"Creating csv file at {output}")
        # Insert header
        res_data.insert(0, res_header)
        with open(output, 'w', newline='') as f:
            writer = csv.writer(f)
            writer.writerows(res_data)

    def line_usage(self, input: str="./coverage.json", output: str="./out_lines.csv", src_code: str=None, sort: SortT="asc"):
        (res_data, res_header, _) = JsonAnalyzer.get_line_data(input, src_code, sort)

        logger.info(f"Creating csv file at {output}")
        # Insert header
        res_data.insert(0, res_header)
        with open(output, 'w', newline='') as f:
            writer = csv.writer(f)
            writer.writerows(res_data)

class Db:
    KIND_TABLE_MAPPING = {
        "line": "LineUsage",
        "func": "FunctionUsage",
        "fline": "FunctionLineUsage"
        }

    def generate(self, input: str="./coverage.json", output: str="./coverage_db.sqlite", src_code: str=None):
        # Connect to DB
        conn = sqlite3.connect(output)
        cur = conn.cursor()

        # Generate a DB table for all analyzer results
        for (kind, table_name) in self.KIND_TABLE_MAPPING.items():
            logger.info(f"Fetching data for {kind} usage")
            (data, header, headers_types) = JsonAnalyzer.get_data(kind, input, src_code, sort=False)

            # Create the table
            logger.info(f"Creating table {table_name}")

            table_fields = ", ".join( 
                [ f"{h} {t}" for (h, t) in zip(header, headers_types) ]
            )
            query = f"CREATE TABLE IF NOT EXISTS {table_name} ({table_fields})"
            logger.debug(query)
            cur.execute(query)

            # Insert all datapoints
            logger.info(f"Inserting table data")

            table_fields = ", ".join(header)
            table_placeholder = ", ".join(["?" for _ in header])
            query = f"INSERT INTO {table_name} ({table_fields}) VALUES ({table_placeholder})"
            logger.debug(query)
            cur.executemany(query, data)

        logger.info(f"Cleaning up")
        conn.commit()
        conn.close()

class Plotter:
    def _read_from_db(self, file: str, kind: KindT, cutoff: int = 0):
        if not os.path.exists(file):
            logger.critical(f"No database file found at '{file}'.")
            logger.info(f"Please create a database first using '{sys.argv[0]} db generate'")
            exit(1)

        conn = sqlite3.connect(file)
        cur = conn.cursor()

        # Get the table_name
        table_name = Db.KIND_TABLE_MAPPING[kind]

        # Extract the column_names
        cur.execute(f"PRAGMA table_info({table_name})")
        column_names = [col[1] for col in cur.fetchall()]

        # Get all entries
        query = f"SELECT * FROM {table_name}"
        if cutoff:
            query += f" WHERE execution_count > {cutoff}"
        cur.execute(query)
        data = cur.fetchall()

        return (data, column_names)


    def line_usage(self, db_file: str="./coverage_db.sqlite", output: str=None, cutoff: int = None, log_scale: bool = False, percentile_categories: bool = False):
        (res_data, res_header) = self._read_from_db(db_file, "line",  cutoff)
        df = pd.DataFrame(res_data, columns=res_header)

        if percentile_categories:
            percent_categories = [0.3, 1, 2]
            logger.info(f"Creating percent categories ({percent_categories})")

            # Categories
            df_cats = [df]
            categories = []
            for p in percent_categories:
                df_first = df.iloc[:int(len(df) * p/100)].copy()
                cname = f'Top {p} percentile'
                df_first['category'] = cname
                categories.append(cname)
                df_cats.append(df_first)

            df['category'] = 'all'
            categories.append('all')

            df_combined = pd.concat(df_cats)
            df_combined['category'] = pd.Categorical(df_combined['category'], categories=categories, ordered=True)
        else:
            df['category'] = 'all'
            df_combined = df

        colors = ['red', 'green', 'blue', 'purple', 'magenta', 'yellow', 'black', 'red', 'red', 'red', 'red', 'red', 'red']
        quantiles_s = [0.99, 0.95, 0.90, 0.80, 0.70, 0.60, 0.50, 0.25, 0.20, 0.15, 0.10, 0.05, 0.01]
        logger.info(f"Creating quantile lines ({[f'{round(100*q)}%' for q in quantiles_s]})")
        quantiles_v = df['execution_count'].quantile(quantiles_s)
        quantiles = pd.DataFrame({
            'Percentile': [f'{(100*q):02.0f} Percentile (y={round(quantiles_v[q])}) (Below: {round((df['execution_count'] < quantiles_v[q]).sum())})' for q in quantiles_s],
            'y': quantiles_v,
            'color': colors[:len(quantiles_s)]
        })

        logger.info(f"Creating plot")
        title ="Distribution of Line Accesses"
        if cutoff is not None:
            title += f" (Count >= {cutoff})"
        plot = (
            ggplot(df_combined, aes(x=df_combined.index, y='execution_count')) +
            geom_point(color="steelblue") +
            geom_hline(quantiles, aes(yintercept='y', color='Percentile'), linetype="dashed", size=1) +
            facet_wrap('~category', scales='free_x') +
            theme_minimal() +
            labs(title=title , x="Index", y="Execution Count") +
            scale_color_manual(values=dict(zip(quantiles['Percentile'], quantiles['color'])))
        )
        if log_scale:
            plot = plot + scale_y_log10()

        if output:
            logger.info(f"Storing plot at {output}")
            plot.save(output, width=8, height=6, dpi=300)
        else:
            logger.info(f"Opening plot preview")
            plot.show()



    def func_usage(self, db_file: str="./coverage_db.sqlite", output: str=None, cutoff: int = None, log_scale: bool = False, percentile_categories: bool = False):
        (res_data, res_header) = self._read_from_db(db_file, "func",  cutoff)
        df = pd.DataFrame(res_data, columns=res_header)

        if percentile_categories:
            percent_categories = [0.3, 1, 2]
            logger.info(f"Creating percent categories ({percent_categories})")

            # Categories
            df_cats = [df]
            categories = []
            for p in percent_categories:
                df_first = df.iloc[:int(len(df) * p/100)].copy()
                cname = f'Top {p} percentile'
                df_first['category'] = cname
                categories.append(cname)
                df_cats.append(df_first)

            df['category'] = 'all'
            categories.append('all')

            df_combined = pd.concat(df_cats)
            df_combined['category'] = pd.Categorical(df_combined['category'], categories=categories, ordered=True)
        else:
            df['category'] = 'all'
            df_combined = df

        colors = ['red', 'green', 'blue', 'purple', 'magenta', 'yellow', 'black', 'red', 'red', 'red', 'red', 'red', 'red']
        quantiles_s = [0.99, 0.95, 0.90, 0.80, 0.70, 0.60, 0.50, 0.25, 0.20, 0.15, 0.10, 0.05, 0.01]
        logger.info(f"Creating quantile lines ({[f'{round(100*q)}%' for q in quantiles_s]})")
        quantiles_v = df['execution_count'].quantile(quantiles_s)
        quantiles = pd.DataFrame({
            'Percentile': [f'{(100*q):02.0f} Percentile (y={round(quantiles_v[q])}) (Below: {round((df['execution_count'] < quantiles_v[q]).sum())})' for q in quantiles_s],
            'y': quantiles_v,
            'color': colors[:len(quantiles_s)]
        })

        logger.info(f"Creating plot")
        title ="Distribution of Function Accesses"
        if cutoff is not None:
            title += f" (Count >= {cutoff})"
        plot = (
            ggplot(df_combined, aes(x=df_combined.index, y='execution_count')) +
            geom_point(color="steelblue") +
            geom_hline(quantiles, aes(yintercept='y', color='Percentile'), linetype="dashed", size=1) +
            facet_wrap('~category', scales='free_x') +
            theme_minimal() +
            labs(title=title , x="Index", y="Execution Count") +
            scale_color_manual(values=dict(zip(quantiles['Percentile'], quantiles['color'])))
        )
        if log_scale:
            plot = plot + scale_y_log10()

        if output:
            logger.info(f"Storing plot at {output}")
            plot.save(output, width=8, height=6, dpi=300)
        else:
            logger.info(f"Opening plot preview")
            plot.show()

    def func_len(self, db_file: str="./coverage_db.sqlite", output: str=None, cutoff: int = None, log_scale: bool = True, percentile_categories: bool = False, group_depth: int = 2):
        (res_data, res_header) = self._read_from_db(db_file, "fline", cutoff)
        df = pd.DataFrame(res_data, columns=res_header)

        # Ensure the data is sorted by func_name and func_no_lines
        df = df.sort_values(by=['file', 'func_name', 'func_no_lines'])

        df["parent folder"] = df["file"].apply(_shorten_path_cblk(group_depth))

        logger.info(f"Creating plot")
        title ="Distribution of Function Lengths"
        if cutoff is not None:
            title += f" (Count >= {cutoff})"
        plot = (
            ggplot(df, aes(x='file', y='func_no_lines', color="parent folder")) +
            geom_point() +
            theme_minimal() +
            labs(title=title , x="File", y="No Lines of Function")
        )
        if log_scale:
            plot = plot + scale_y_log10()

        if output:
            logger.info(f"Storing plot at {output}")
            plot.save(output, width=8, height=6, dpi=300)
        else:
            logger.info(f"Opening plot preview")
            plot.show()

    def fline_len(self, db_file: str="./coverage_db.sqlite", output: str=None, cutoff: int = None, log_scale: bool = True, percentile_categories: bool = False, group_depth: int = 2):
        (res_data, res_header) = self._read_from_db(db_file, "fline", cutoff)
        df = pd.DataFrame(res_data, columns=res_header)

        # Ensure the data is sorted by func_name and func_no_lines
        df = df.sort_values(by=['file', 'func_name', 'func_no_lines'])

        df["parent folder"] = df["file"].apply(_shorten_path_cblk(group_depth))

        logger.info(f"Creating plot")
        title ="Distribution of Function Length against Executions"
        if cutoff is not None:
            title += f" (Count >= {cutoff})"
        plot = (
            ggplot(df, aes(x='execution_count', y='func_no_lines', color="parent folder")) +
            geom_point() +
            theme_minimal() +
            labs(title=title , x="Execution Count", y="No Lines of Function")
        )
        if log_scale:
            plot = plot + scale_y_log10()

        if output:
            logger.info(f"Storing plot at {output}")
            plot.save(output, width=8, height=6, dpi=300)
        else:
            logger.info(f"Opening plot preview")
            plot.show()

    def fline_usage(self, db_file: str="./coverage_db.sqlite", output: str=None, cutoff: int = None, log_scale: bool = False, percentile_categories: bool = False, group_depth: int = 2):
        (res_data, res_header) = self._read_from_db(db_file, "fline", cutoff)
        ["uid", "execution_count", "file", "func_name", "line_no", "func_no_lines"]
        df = pd.DataFrame(res_data, columns=res_header)

        # Ensure the data is sorted by func_name and func_no_lines
        df = df.sort_values(by=['file', 'func_name', 'line_no'])

        df["fid"] = df["file"] + "::" + df["func_name"]
        df["execution count diff"] = df["execution_count"].diff()

        df["parent_folder"] = df["file"].apply(_shorten_path_cblk(group_depth))

        logger.info(f"Creating plot")
        title ="Distribution of Line Accesses per Function"
        if cutoff is not None:
            title += f" (Count >= {cutoff})"
        plot = (
            ggplot(df, aes(x='line_no', y='execution_count', color="fid")) +
            geom_line() +
            theme_minimal() +
            theme(legend_position='none') +
            facet_wrap('~parent_folder', scales='free') +
            labs(title=title , x="Line of Function", y="Execution Count")
        )
        if log_scale:
            plot = plot + scale_y_log10()

        if output:
            logger.info(f"Storing plot at {output}")
            plot.save(output, width=8, height=6, dpi=300)
        else:
            logger.info(f"Opening plot preview")
            plot.show()




class Analyzer:
    """ Analyze coverage.json experiment reports """

    def csv(self):
        return Csv()

    def db(self):
        return Db()

    def plot(self):
        return Plotter()



if __name__ == "__main__":
    fire.Fire(Analyzer)
