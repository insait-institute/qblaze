import pytest
import gc
import os
import sys
import time
import typing

import qblaze


def test_iter_simple():
    sim = qblaze.Simulator()
    it = iter(sim)
    assert iter(it) is it
    assert next(it) == (0, 1.0)
    with pytest.raises(StopIteration):
        next(it)
    with pytest.raises(StopIteration):
        next(it)


def test_iter_invalidate() -> None:
    sim = qblaze.Simulator()
    it = iter(sim)
    sim.x(0)
    with pytest.raises(RuntimeError):
        next(it)
    with pytest.raises(RuntimeError):
        next(it)
    it2 = iter(sim)
    sim.h(0)
    it3 = iter(sim)
    with pytest.raises(RuntimeError):
        next(it2)
    assert len(list(it3)) == 2


def round_one(p: tuple[int, complex]) -> tuple[int, complex]:
    (i, v) = p
    return (i, complex(round(v.real, 8), round(v.imag, 8)))


def round_list(l: typing.Iterable[tuple[int, complex]]) -> list[tuple[int, complex]]:
    return [round_one(p) for p in l]


def test_iter_list() -> None:
    sim = qblaze.Simulator()
    sim.h(0)
    sim.h(1)
    sim.ccx(0, 1, 2)
    sim.z(2)
    it = iter(sim)
    assert round_list(list(it)) == [(0, 0.5), (1, 0.5+0j), (2, 0.5+0j), (7, -0.5)]


def test_iter_multiple() -> None:
    sim = qblaze.Simulator()
    sim.h(0)
    sim.h(1)
    sim.ccx(0, 1, 2)
    sim.z(2)

    it1 = iter(sim)

    it2 = iter(sim)
    assert round_one(next(it2)) == (0, 0.5)
    with pytest.raises(RuntimeError):
        next(it1)
    assert round_one(next(it2)) == (1, 0.5)
    assert round_one(next(it2)) == (2, 0.5)

    it3 = iter(sim)
    assert round_one(next(it3)) == (0, 0.5)
    assert round_one(next(it3)) == (1, 0.5)
    with pytest.raises(RuntimeError):
        next(it2)
    assert round_one(next(it3)) == (2, 0.5)
    assert round_one(next(it3)) == (7, -0.5)


def test_iter_keeps_ref() -> None:
    it = iter(qblaze.Simulator())
    gc.collect()
    assert len(list(it)) == 1


@pytest.mark.skipif(sys.platform != 'linux', reason='/proc/self/task')
def test_iter_releases_ref() -> None:
    init_threads = frozenset(os.listdir('/proc/self/task'))
    it = iter(qblaze.Simulator(thread_count=16))
    gc.collect()

    new_threads = frozenset(os.listdir('/proc/self/task')) - init_threads
    assert len(new_threads) == 15

    del it
    gc.collect()
    for i in range(5):
        if frozenset(os.listdir('/proc/self/task')).isdisjoint(new_threads):
            break
        time.sleep(0.1)
    assert frozenset(os.listdir('/proc/self/task')).isdisjoint(new_threads)
