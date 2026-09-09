import math
import cmath
import numpy
import qblaze

import pytest


TESTS = [
    (1, [(0, 1j)]),
    (4, [(1, 1j)]),
    (6000, [(1, 1j / 2**0.5), (5000, 1 / 2**0.5)]),
    (2**20, [((i << 8) + (i * 37 % 256), cmath.exp(2j * math.pi * i / 2**12) / 2**6) for i in range(2**12)]),
    (2**20, [(2**20-4, 0.5), (2**20-3, -0.5j), (2**20-2, -0.5), (2**20-1, 0.5j)]),
]


@pytest.mark.parametrize("n, values", TESTS)
def test_import(n, values):
    state = numpy.zeros(n, dtype=numpy.complex128)
    for (i, v) in values:
        state[i] = v
    sim = qblaze.Simulator()
    sim.import_amplitudes(state)
    assert list(sim) == values
