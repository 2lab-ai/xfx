"""LOCAL-PATHS-WRAP round 4: whether scripts/fxcap/roots.py's code is the same
at two revisions -- its syntax tree with every docstring emptied (a comment
is not in the tree), compared whole. Prints the verdict and the line counts
of the diff, never a line.

usage: python3 -I same_rule.py <repo> <rev a> <rev b>
"""
import ast
import subprocess
import sys

repo, a, b = sys.argv[1:4]
env_free = {"PATH": "/usr/bin:/bin"}


def source(rev):
    return subprocess.run(["git", "-C", repo, "show", rev + ":scripts/fxcap/roots.py"], capture_output=True, text=True,
                          check=True, env=env_free).stdout


def code(text):
    tree = ast.parse(text)
    for node in ast.walk(tree):
        body = getattr(node, "body", None)
        if isinstance(node, (ast.Module, ast.FunctionDef, ast.ClassDef)) and body and isinstance(body[0], ast.Expr) \
                and isinstance(body[0].value, ast.Constant) and isinstance(body[0].value.value, str):
            body[0].value.value = ""
    return ast.dump(tree)


sa, sb = source(a), source(b)
stat = subprocess.run(["git", "-C", repo, "diff", "--numstat", a, b, "--", "scripts/fxcap/roots.py"], capture_output=True,
                      text=True, check=True, env=env_free).stdout.split()
print("roots.py %s -> %s: %s lines added, %s removed; its code without docstrings and comments is %s"
      % (a, b, stat[0] if stat else 0, stat[1] if stat else 0, "the same" if code(sa) == code(sb) else "DIFFERENT"))
