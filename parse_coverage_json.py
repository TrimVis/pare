import fire
import orjson
import csv
import os
import pandas as pd
from loguru import logger 
from plotnine import ggplot, aes, geom_point, theme_minimal, labs, theme, element_blank, facet_wrap, geom_hline, annotate

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

    def csv(self, input: str="./coverage.json", output: str="./out.csv", src_code: str=None, sort: SortT="asc", cutoff: int = 0):
        res_header = ["uid", "execution count", "file", "func_name"]
        res_data = self._get_usage_data(input, src_code, sort, cutoff)

        logger.info(f"Creating csv file at {output}")
        # Insert header
        res_data.insert(0, res_header)
        with open(output, 'w', newline='') as f:
            writer = csv.writer(f)
            writer.writerows(res_data)

    def plot(self, input: str="./coverage.json", output: str=None, src_code: str=None, sort: SortT="asc", cutoff: int = None):
        res_data = self._get_usage_data(input, src_code, sort, cutoff)
        df = pd.DataFrame(res_data, columns=['uid', 'exec_count', 'cleaned_path', 'func_name'])

        percent_categories = [0.3, 1, 2]
        logger.info(f"Creating percent categories ({percent_categories})")

        # Categories
        df_cats = [df]
        categories = []
        for p in percent_categories:
            df_first = df.iloc[:int(len(df) * p/100)].copy()
            cname = f'first_{p}'
            df_first['category'] = cname
            categories.append(cname)
            df_cats.append(df_first)

        df['category'] = 'all'
        categories.append('all')

        df_combined = pd.concat(df_cats)
        df_combined['category'] = pd.Categorical(df_combined['category'], categories=categories, ordered=True)

        colors = ['red', 'green', 'blue', 'purple', 'magenta']
        quantiles_s = [0.999, 0.99, 0.95, 0.90]
        logger.info(f"Creating quantile lines ({[f'{100*q}%' for q in quantiles_s]})")
        quantiles = df['exec_count'].quantile(quantiles_s)

        logger.info(f"Creating plot")
        plot = (
            ggplot(df_combined, aes(x=df_combined.index, y='exec_count')) +
            geom_point(color="steelblue")
        )
        for (i, l) in enumerate(quantiles_s):
            color = colors[i]
            plot = ( 
                    plot 
                    + geom_hline(yintercept=quantiles[l], linetype="dashed", color=color, size=1) 
                    + annotate('text', x=5, y=quantiles[l] + 5, label=f"{100*l} Percentile", color=color, ha='left')
            )

        plot = (
            plot
            + facet_wrap('~category', scales='free_x')
            + theme_minimal()
            + labs(title="Distribution of Function Accesses", x="Index", y="Execution Count")
        )

        if output:
            logger.info(f"Storing plot at {output}")
            plot.save(output, width=8, height=6, dpi=300)
        else:
            logger.info(f"Opening plot preview")
            plot.show()


class FuncLineAnalyzer:
    def _get_usage_data(self, input: str, src_code: str, sort: SortT):
        d = _read_json(input)
        src_code = src_code.rstrip("/").rstrip("src")

        res_data = [  ]

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
            for (line_no, exec_count) in lines.items():
                # Check if line falls into next function
                if curr_func_i < (len(func_line_map) - 1) and int(line_no) >= int(func_line_map[curr_func_i][3]):
                    curr_func_i += 1

                (func_path, func_id, func_name, func_start) = func_line_map[curr_func_i]
                uid = f"{func_path}:{func_id}:l{line_no}"
                res_data.append(
                    [uid, exec_count, func_path, func_name, line_no]
                )

        return res_data

    def csv(self, input: str="./coverage.json", output: str="./out.csv", src_code: str=None, sort: SortT=False):
        res_header = ["uid", "execution count", "file", "func_name", "line_no"]
        res_data = self._get_usage_data(input, src_code, sort)

        # Insert header
        res_data.insert(0, res_header)
        with open(output, 'w', newline='') as f:
            writer = csv.writer(f)
            writer.writerows(res_data)

    def plot(self, input: str="./coverage.json", output: str="./out.png", src_code: str=None, sort: SortT="asc"):
        res_data = self._get_usage_data(input, src_code, sort)
        df = pd.DataFrame(res_data, columns=['uid', 'exec_count', 'cleaned_path', 'func_name'])

        percent_categories = [0.3, 1, 2]
        # Categories

        df_cats = [df]
        categories = []
        for p in percent_categories:
            df_first = df.iloc[:int(len(df) * p/100)].copy()
            cname = f'first_{p}'
            df_first['category'] = cname
            categories.append(cname)
            df_cats.append(df_first)

        df['category'] = 'all'
        categories.append('all')

        df_combined = pd.concat(df_cats)
        df_combined['category'] = pd.Categorical(df_combined['category'], categories=categories, ordered=True)

        plot = (
            ggplot(df_combined, aes(x=df_combined.index, y='exec_count')) +
            geom_point(color="steelblue") +
            facet_wrap('~category', scales='free_x') +
            theme_minimal() +
            labs(title="Distribution of Function Accesses", x="Index", y="Execution Count")
        )
        plot.show()




class Analyzer:
    """ Analyze coverage.json experiment reports """

    def func_usage(self):
        return FuncAnalyzer()

    def fline_usage(self):
        return FuncLineAnalyzer()



if __name__ == "__main__":
    fire.Fire(Analyzer)
