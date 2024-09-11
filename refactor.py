#!/usr/bin/env python

import fire
import tree_sitter_cpp as tscpp
from tree_sitter import Language, Parser

RED = "\033[31m"
GREEN = "\033[32m"
YELLOW = "\033[33m"
BLUE = "\033[34m"
RESET = "\033[0m"

CPP_LANGUAGE = Language(tscpp.language())
parser = Parser(CPP_LANGUAGE)

def _get_tree_from_file(file):
    with open(file, 'rb') as content:
        return parser.parse(content.read(), encoding="utf8")

class RefactorEngine:
    def analyze_core(self):
        tree = _get_tree_from_file("./example.cpp")
        root_node = tree.root_node

        line_execution_map = { 14: 100, 8: 100 }
        results = []

        def traverse_tree(node):
            print(f"|==> {node.type} - start: {node.start_point[0] + 1},{node.start_point[1] + 1}; end {node.end_point[0] + 1},{node.end_point[1] + 1}")
            print(f"{GREEN}{node}{RESET}")
            if node.type in ['if_statement', 'for_statement', 'while_statement', 'compound_statement']:
                start_line = node.start_point[0] + 1  # Tree-sitter is zero-indexed
                end_line = node.end_point[0] + 1
                
                # Testing placeholder
                exec_count = sum(line_execution_map.get(line, 0) for line in range(start_line, end_line + 1))
                if exec_count < 2:
                    print(f"{RED}Found a rarely used branch!{RESET}")
                    results.append((start_line, end_line))

            print()

            for child in node.children:
                traverse_tree(child)

        traverse_tree(root_node)

        print("")
        for (start_line, end_line) in results:
            print(f"Commenting out branch at lines {start_line}-{end_line}")


        pass

if __name__ == "__main__":
    fire.Fire(RefactorEngine)
