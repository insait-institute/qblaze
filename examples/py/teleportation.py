import math
import random
import typing

import numpy

import qblaze


def init_random(sim: qblaze.Simulator, target: int) -> None:
    """Initialize a |0> qubit to a random (non-entangled) state."""
    sim.ry(target, random.random() * 2 * math.pi)
    sim.rz(target, random.random() * 2 * math.pi)


def init_bell_pair(sim: qblaze.Simulator, target1: int, target2: int) -> None:
    """Initialize a |00> pair to the GHZ state (|00> + |11>) / sqrt(2)."""
    sim.h(target1)
    sim.cx(target1, target2)


def teleport_send(sim: qblaze.Simulator, source: int, bell_local: int) -> tuple[bool, bool]:
    """Given one element of a bell pair, send the local qubit and return the
    classical data necessary to receive it. Resets the qubits."""

    sim.cx(source, bell_local)
    sim.h(source)
    c1 = sim.measure(source)
    if c1:
        sim.x(source)
    c2 = sim.measure(bell_local)
    if c2:
        sim.x(bell_local)
    return (c1, c2)


def teleport_recv(sim: qblaze.Simulator, target: int, data: tuple[bool, bool]) -> None:
    """Receive a qubit from the data returned by `teleport_send` and the other
    element of the bell pair."""
    (c1, c2)  = data
    if c1:
        sim.z(target)
    if c2:
        sim.x(target)


def almost_equal_up_to_global_phase(sv1: numpy.ndarray[tuple[int], numpy.dtype[numpy.complex128]], sv2: numpy.ndarray[tuple[int], numpy.dtype[numpy.complex128]]) -> bool:
    assert len(sv1) == len(sv2)

    # First compare absolute values
    if numpy.abs(numpy.abs(sv1) - numpy.abs(sv2)).max() >= 1e-6:
        return False

    # Find the relative phase between the two state vectors.
    # abs(sv1[i]) is large because it is the largest value, and because of the
    # check above abs(sv2[i]) is similar. This way we avoid dividing by zero.
    i = numpy.abs(sv1).argmax()
    v: numpy.complex128 = sv1[i] / sv2[i]
    v /= abs(v)
    diff: numpy.float64 = numpy.abs(sv1 - sv2 * v).max()
    return not not (diff < 1e-6)


def main() -> None:
    # Prepare two simulators in the same state.
    sim1 = qblaze.Simulator()
    init_random(sim1, 0)
    sim2 = sim1.clone()

    # Do teleportation in one.
    init_bell_pair(sim1, 1, 2)
    data = teleport_send(sim1, 0, 1)
    teleport_recv(sim1, 2, data)

    # Do a plain swap in the other.
    sim2.swap(0, 2)

    # Compare the state vectors
    sv1 = numpy.zeros(8, numpy.complex128)
    sim1.copy_amplitudes(sv1)
    sv2 = numpy.zeros(8, numpy.complex128)
    sim2.copy_amplitudes(sv2)
    assert almost_equal_up_to_global_phase(sv1, sv2)

    print(sv1[0], sv1[4])
    print(sv2[0], sv2[4])
    for i in (1, 2, 3, 5, 6, 7):
        assert abs(sv1[i]) < 1e-6
        assert abs(sv2[i]) < 1e-6


main()
