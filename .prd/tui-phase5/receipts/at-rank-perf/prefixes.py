"""How many searches `the_pruned_search_finds_what_at_ranks_full_search_finds`
compares: its `queries` array read from the source, each query's scalars
counted (one search per prefix, the whole query the last), on each of its
two corpora.

usage: python3 -I prefixes.py   (run in the repo)
"""
import re
import sys

sys.dont_write_bytecode = True
src = open("src/tui/file_index.rs", encoding="utf-8").read()
body = src[src.index("fn the_pruned_search_finds_what_at_ranks_full_search_finds"):]
array = body[body.index("let queries = [") : body.index("];", body.index("let queries = ["))]
queries = [re.sub(r"\\u\{([0-9a-f]+)\}", lambda m: chr(int(m[1], 16)), q) for q in re.findall(r'"((?:[^"\\]|\\.)*)"', array)]
n = sum(len(q) for q in queries)
print(f"queries: {len(queries)}; prefixes: {n}; corpora: 2; searches: {2 * n}")
