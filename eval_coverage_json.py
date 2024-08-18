#!/usr/bin/env python

import fire
import orjson
import csv
import os
import pandas as pd
from loguru import logger 
from plotnine import ggplot, aes, geom_point, theme_minimal, labs, theme, element_blank, facet_wrap, geom_hline, annotate, scale_color_manual, scale_y_log10, geom_line

from typing import Literal

def _read_json(file: str):
    with open(file, "rb") as f:
        data = f.read()
    return orjson.loads(data)


def _lookup_function_name(src_code: str, file_path: str, line_no: int) -> str:
    if not src_code:
        return "-"

    
    full_path = os.path.join(src_code, file_path)
    with open(full_path, 'r') as f:
        for i, line in enumerate(f, start=1):
            if i == line_no:
                return line.strip().rstrip("{")

    # Not enough line numbers in this file
    return "-"

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


SortT = Literal["ASC"] | Literal["DESC"] | bool

class LineAnalyzer:
    def _get_usage_data(self, input: str, src_code: str, sort: SortT, cutoff: int):
        logger.info(f"Reading usage data from {input}")
        d = _read_json(input)
        src_code = src_code.rstrip("/").rstrip("src")

        res_data = [  ]

        lines_analyzed = 0
        lines_skipped = 0

        for (path, value) in d["sources"].items():
            # there is always a object with only the empty key
            value = value[""]
            lines = value["lines"]
            cleaned_path = _clean_path(path)
            for (line_no, exec_count) in lines.items():
                if cutoff is not None and int(exec_count) <= cutoff:
                    lines_skipped += 1
                    continue

                lines_analyzed += 1
                uid = f"{cleaned_path}:{line_no}"
                res_data.append(
                    [uid, exec_count, cleaned_path, line_no]
                )

        logger.info(f"Usage data contains {lines_analyzed} of {lines_analyzed + lines_skipped} lines. ({100 * lines_analyzed / (lines_analyzed + lines_skipped)}%)")
        logger.info(f"Ignored {lines_skipped} lines ({100* lines_skipped / (lines_analyzed + lines_skipped)}%), due to below cutoff ({cutoff}) usage.")

        # Sort the result data, if wanted
        if sort:
            reverse_sort = sort.lower() if isinstance(sort, str) else False
            logger.info(f"Sorting usage data by execution count in {'descending' if reverse_sort else 'ascending'} order")
            key_fn = lambda i: i[1]
            res_data.sort(key=key_fn, reverse=reverse_sort)

        return res_data

    def csv(self, input: str="./coverage.json", output: str="./out_lines.csv", src_code: str=None, sort: SortT="asc", cutoff: int = 0):
        res_header = ["uid", "execution count", "file_path", "line_no"]
        res_data = self._get_usage_data(input, src_code, sort, cutoff)

        logger.info(f"Creating csv file at {output}")
        # Insert header
        res_data.insert(0, res_header)
        with open(output, 'w', newline='') as f:
            writer = csv.writer(f)
            writer.writerows(res_data)

    def plot(self, input: str="./coverage.json", output: str=None, src_code: str=None, sort: SortT="asc", cutoff: int = None, log_scale: bool = False, percentile_categories: bool = False):
        res_data = self._get_usage_data(input, src_code, sort, cutoff)
        df = pd.DataFrame(res_data, columns=['uid', 'exec_count', 'file', 'line_no'])

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

        colors = ['red', 'green', 'blue', 'purple', 'magenta', 'yellow', 'black']
        quantiles_s = [0.99, 0.95, 0.90, 0.50, 0.10, 0.05, 0.01]
        logger.info(f"Creating quantile lines ({[f'{round(100*q)}%' for q in quantiles_s]})")
        quantiles_v = df['exec_count'].quantile(quantiles_s)
        quantiles = pd.DataFrame({
            'Percentile': [f'{(100*q):02.0f} Percentile (y={round(quantiles_v[q])}) (Below: {round((df['exec_count'] < quantiles_v[q]).sum())})' for q in quantiles_s],
            'y': quantiles_v,
            'color': colors[:len(quantiles_s)]
        })

        logger.info(f"Creating plot")
        title ="Distribution of Line Accesses"
        if cutoff is not None:
            title += f" (Count >= {cutoff})"
        plot = (
            ggplot(df_combined, aes(x=df_combined.index, y='exec_count')) +
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

class FuncAnalyzer:
    def _get_usage_data(self, input: str, src_code: str, sort: SortT, cutoff: int):
        logger.info(f"Reading usage data from {input}")
        d = _read_json(input)
        src_code = src_code.rstrip("/").rstrip("src")

        res_data = [  ]

        funcs_analyzed = 0
        funcs_skipped = 0

        for (path, value) in d["sources"].items():
            # there is always a object with only the empty key
            value = value[""]
            functions = value["functions"]
            branches = value["branches"]
            lines = value["lines"]
            cleaned_path = _clean_path(path)
            for (func_id, func_value) in functions.items():
                exec_count = func_value["execution_count"]
                if cutoff is not None and exec_count <= cutoff:
                    funcs_skipped += 1
                    continue

                funcs_analyzed += 1
                func_name = _lookup_function_name(src_code, cleaned_path, func_value["start_line"])
                uid = f"{cleaned_path}:{func_id}"
                res_data.append(
                    [uid, exec_count, cleaned_path, func_name]
                )

        logger.info(f"Usage data contains {funcs_analyzed} of {funcs_analyzed + funcs_skipped} functions. ({100 * funcs_analyzed / (funcs_analyzed + funcs_skipped)}%)")
        logger.info(f"Ignored {funcs_skipped} functions ({100* funcs_skipped / (funcs_analyzed + funcs_skipped)}%), due to below cutoff ({cutoff}) usage.")

        # Sort the result data, if wanted
        if sort:
            reverse_sort = sort.lower() if isinstance(sort, str) else False
            logger.info(f"Sorting usage data by function execution count in {'descending' if reverse_sort else 'ascending'} order")
            key_fn = lambda i: i[1]
            res_data.sort(key=key_fn, reverse=reverse_sort)

        return res_data

    def csv(self, input: str="./coverage.json", output: str="./out_functions.csv", src_code: str=None, sort: SortT="asc", cutoff: int = 0):
        res_header = ["uid", "execution count", "file", "func_name"]
        res_data = self._get_usage_data(input, src_code, sort, cutoff)

        logger.info(f"Creating csv file at {output}")
        # Insert header
        res_data.insert(0, res_header)
        with open(output, 'w', newline='') as f:
            writer = csv.writer(f)
            writer.writerows(res_data)

    def plot(self, input: str="./coverage.json", output: str=None, src_code: str=None, sort: SortT="asc", cutoff: int = None, log_scale: bool = False, percentile_categories: bool = False):
        res_data = self._get_usage_data(input, src_code, sort, cutoff)
        df = pd.DataFrame(res_data, columns=['uid', 'exec_count', 'cleaned_path', 'func_name'])

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

        colors = ['red', 'green', 'blue', 'purple', 'magenta', 'yellow', 'black']
        quantiles_s = [0.99, 0.95, 0.90, 0.50, 0.10, 0.05, 0.01]
        logger.info(f"Creating quantile lines ({[f'{round(100*q)}%' for q in quantiles_s]})")
        quantiles_v = df['exec_count'].quantile(quantiles_s)
        quantiles = pd.DataFrame({
            'Percentile': [f'{(100*q):02.0f} Percentile (y={round(quantiles_v[q])}) (Below: {round((df['exec_count'] < quantiles_v[q]).sum())})' for q in quantiles_s],
            'y': quantiles_v,
            'color': colors[:len(quantiles_s)]
        })

        logger.info(f"Creating plot")
        title ="Distribution of Function Accesses"
        if cutoff is not None:
            title += f" (Count >= {cutoff})"
        plot = (
            ggplot(df_combined, aes(x=df_combined.index, y='exec_count')) +
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


class FuncLineAnalyzer:
    def _get_usage_data(self, input: str, src_code: str, sort: SortT, cutoff: int, relevance_filter: float):
        d = _read_json(input)
        src_code = src_code.rstrip("/").rstrip("src")

        res_data = [  ]
        
        lines_skipped = 0
        lines_analyzed = 0
        relevant_functions = 0
        non_relevant_functions = 0

        for (path, value) in d["sources"].items():
            # there is always a object with only the empty key
            value = value[""]
            functions = value["functions"]
            branches = value["branches"]
            lines = value["lines"]
            cleaned_path = _clean_path(path)

            func_line_map = []
            for (func_id, func_value) in functions.items():
                func_name = _lookup_function_name(src_code, cleaned_path, func_value["start_line"])
                func_line_map.append(
                    (cleaned_path, func_id, func_name, func_value["start_line"])
                )

            curr_func_i = 0
            prev_count = None
            relevant = False
            f_data = []
            for (line_no, exec_count) in lines.items():
                if cutoff is not None and exec_count <= cutoff:
                    lines_skipped += 1
                    continue
                lines_analyzed += 1

                # Check if line falls into next function
                if curr_func_i < (len(func_line_map) - 1) and int(line_no) >= int(func_line_map[curr_func_i][3]):
                    prev_count = None
                    if relevance_filter:
                        if relevant:
                            res_data.extend( f_data)
                            f_data = []
                            relevant_functions += 1
                        else:
                            non_relevant_functions += 1
                        relevant = False
                    curr_func_i += 1

                if prev_count is not None and relevance_filter is not None and exec_count <= relevance_filter * prev_count:
                    relevant = True
                prev_count = exec_count

                (func_path, func_id, func_name, func_start) = func_line_map[curr_func_i]
                uid = f"{func_path}:{func_id}:l{line_no}"
                if not relevance_filter:
                    res_data.append(
                        [uid, exec_count, func_path, func_name, line_no, int(line_no) - func_start]
                    )
                else:
                    f_data.append(
                        [uid, exec_count, func_path, func_name, line_no, int(line_no) - func_start]
                    )
            if relevance_filter:
                if relevant:
                    res_data.extend(f_data)
                    relevant_functions += 1
                else:
                    non_relevant_functions += 1

        logger.info(f"Usage data contains {lines_analyzed} of {lines_analyzed + lines_skipped} lines. ({100 * lines_analyzed / (lines_analyzed + lines_skipped)}%)")
        logger.info(f"Ignored {lines_skipped} lines ({100* lines_skipped / (lines_analyzed + lines_skipped)}%), due to below cutoff ({cutoff}) usage.")

        if relevance_filter is not None:
            logger.info(f"Relevant functions found: {relevant_functions} of {relevant_functions + non_relevant_functions} lines. ({100 * relevant_functions / (relevant_functions + non_relevant_functions)}%)")
            logger.info(f"Ignored {non_relevant_functions} functions ({100* non_relevant_functions / (relevant_functions + non_relevant_functions)}%), due to line-by-line relative change being larger than {100*relevance_filter}%")

        return res_data

    def csv(self, input: str="./coverage.json", output: str="./out_function_lines.csv", src_code: str=None, sort: SortT=False, cutoff: int = None, relevance_filter: float = None):
        res_header = ["uid", "execution count", "file", "func_name", "line_no", "func_line_no"]
        res_data = self._get_usage_data(input, src_code, sort, cutoff, relevance_filter)

        # Insert header
        res_data.insert(0, res_header)
        with open(output, 'w', newline='') as f:
            writer = csv.writer(f)
            writer.writerows(res_data)

    def plot(self, input: str="./coverage.json", output: str=None, src_code: str=None, sort: SortT="asc", cutoff: int = None, log_scale: bool = False, relevance_filter: float=None):
        res_data = self._get_usage_data(input, src_code, sort, cutoff, relevance_filter)
        df = pd.DataFrame(res_data, columns=['uid', 'exec_count', 'file', 'func_name', "line_no", "func_line_no"])

        # Ensure the data is sorted by func_name and func_line_no
        df = df.sort_values(by=['file', 'func_name', 'func_line_no'])

        logger.info(f"Creating plot")
        title ="Distribution of Line Accesses per Function"
        if cutoff is not None:
            title += f" (Count >= {cutoff})"
        plot = (
            ggplot(df, aes(x='func_line_no', y='exec_count', color="func_name")) +
            geom_line() +
            theme_minimal() +
            labs(title=title , x="Function Line No.", y="Execution Count")
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

    def line_usage(self):
        return LineAnalyzer()

    def func_usage(self):
        return FuncAnalyzer()

    def fline_usage(self):
        return FuncLineAnalyzer()



if __name__ == "__main__":
    fire.Fire(Analyzer)
