"""Read Rust items for the run-contract guard without mistaking fixture strings for code."""
import re

TOKEN = re.compile(r'//[^\n]*|/\*.*?\*/|r(?P<hashes>\#*)".*?"(?P=hashes)|"(?:\\.|[^"\\])*"|\'(?:\\.|[^\'\\])\'', re.S)


def masked(source):
    return TOKEN.sub(lambda match: ''.join('\n' if c == '\n' else ' ' for c in match[0]), source)


def test_functions(source):
    clean = masked(source)
    functions = {}
    for match in re.finditer(r'#\[test\]\s*fn\s+(\w+)\s*\([^)]*\)\s*\{', clean):
        depth, end = 1, match.end()
        while depth:
            assert end < len(clean), match[1]
            depth += (clean[end] == '{') - (clean[end] == '}')
            end += 1
        functions[match[1]] = source[match.start():end]
    return functions
