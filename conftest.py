import pytest

import doctest
import math
import os
import pathlib
import re
import types
import typing

import numpy

import qblaze


def normalize_floats(s: str) -> str:
    """Round all things that look like floating-point numbers."""
    r = []
    at = 0
    for m in re.finditer('(?:0|[1-9][0-9]*)\\.[0-9]+', s):
        data = m.group(0)
        val = float(data)
        if data != repr(val):
            # Not a python-formatted float.
            continue
        r.append(s[at:m.start()])
        data = f'{val:.10f}'.rstrip('0')
        if data.endswith('.'):
            data += '0'
        r.append(data)
        at = m.end()
    r.append(s[at:])
    return ''.join(r)


def normalize_signs(s: str) -> str:
    return re.sub('(\\()-(0(?:\\.0)?j?[^0-9])', '\\1\\2', s)


class ApproximateOutputChecker(doctest.OutputChecker):
    def check_output(self, want, got, optionflags):
        got = normalize_floats(got)
        got = normalize_signs(got)
        return super().check_output(want, got, optionflags)

    def output_difference(self, example, got, optionflags):
        got = normalize_floats(got)
        got = normalize_signs(got)
        return super().output_difference(example, got, optionflags)


class DoctestStub(pytest.Module):
    def collect(self) -> typing.Iterable[pytest.DoctestItem]:
        runner = doctest.DebugRunner(
            verbose = False,
            optionflags = doctest.ELLIPSIS,
            checker = ApproximateOutputChecker(),
        )

        rel_path = self.path.relative_to(self.config.rootpath / 'python')
        name, ext = os.path.splitext(rel_path.name)
        mod = '.'.join(rel_path.parent.parts)
        if name != '__init__':
            mod = f'{mod}.{name}_test'
        else:
            mod += '_test'

        module = types.ModuleType(mod)
        exec(self.path.read_text(), module.__dict__)
        for test in doctest.DocTestFinder().find(module):
            if not test.examples:
                continue
            yield pytest.DoctestItem.from_parent(self, name=test.name, runner=runner, dtest=test)


def pytest_collect_file(file_path: pathlib.Path, parent: pytest.Collector) -> pytest.Collector | None:
    if file_path.is_relative_to(parent.config.rootpath / 'python' / 'qblaze') and file_path.suffix in ('.py', '.pyi'):
        return DoctestStub.from_parent(parent, path=file_path)
    return None


@pytest.fixture(autouse=True, scope='function')
def add_doctest_globals(doctest_namespace):
    doctest_namespace['math'] = math
    doctest_namespace['numpy'] = numpy
    doctest_namespace['Simulator'] = qblaze.Simulator
    doctest_namespace['sim'] = qblaze.Simulator()
