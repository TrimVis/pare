import fire
import orjson
import csv

from typing import Literal

SortT = Literal["ASC"] | Literal["DESC"] | bool

class Analyzer:
    """ Analyze coverage.json experiment reports """

    def _read_json(self, file: str):
        with open(file, "rb") as f:
            data = f.read()
        return orjson.loads(data)


    def _lookup_function_name(self, file_path: str, line_no: int) -> str:
        if not self.src_code:
            return "-"

        # TODO pjordan: Add this

    def _clean_path(self, path: str) -> str:
        src_index = path.find("src")
        build_index = path.find("build")

        # Determine the earliest occurrence of either "src" or "build"
        if src_index != -1 and (build_index == -1 or src_index < build_index):
            return path[src_index:]
        elif build_index != -1:
            return path[build_index:]
        
        # If neither "src" nor "build" are found, return the original path
        return path

    def func_usage_csv(self, input: str="./coverage.json", output: str="./out.csv", src_code: str=None, sort: SortT="asc"):
        d = self._read_json(input)
        self.src_code = src_code

        res_header = ["uid", "execution count", "file", "func_name"]
        res_data = [  ]

        for (path, value) in d["sources"].items():
            # there is always a object with only the empty key
            value = value[""]
            functions = value["functions"]
            branches = value["branches"]
            lines = value["lines"]
            cleaned_path = self._clean_path(path)
            for (func_id, func_value) in functions.items():
                uid = f"{cleaned_path}:{func_id}"
                exec_count = func_value["execution_count"]
                func_name = self._lookup_function_name(path, func_value["start_line"])
                res_data.append(
                    [uid, exec_count, cleaned_path, func_name]
                )

        # Sort the result data, if wanted
        if sort:
            reverse_sort = sort.lower() if isinstance(sort, str) else False
            key_fn = lambda i: i[1]
            res_data.sort(key=key_fn, reverse=reverse_sort)

        with open(output, 'w', newline='') as f:
            # Insert header
            res_data.insert(0, res_header)
            writer = csv.writer(f)
            writer.writerows(res_data)


if __name__ == "__main__":
    fire.Fire(Analyzer)
