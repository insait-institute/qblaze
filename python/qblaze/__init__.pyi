# SPDX-License-Identifier: Apache-2.0
from __future__ import annotations
import collections
import typing


__version__: typing.Final[str] = '...'

# Hack to make Sphinx discover `qblaze.qiskit`
if '__file__' in globals():
    __path__ = [__import__('os').path.dirname(__file__)]


class Simulator:
    """Simulator() -> Simulator

    Create a new simulator object.

    The simulator supports :code:`Simulator.max_qubit_count()` qubits, which are initially all :code:`0`.

    Example:

        >>> from qblaze import Simulator
        >>> sim = Simulator()

    Read the amplitudes of the :math:`\\ket{00\\ldots}` and :math:`\\ket{10\\ldots}` basis states:

        >>> state = numpy.zeros(2**1, numpy.complex128)
        >>> sim.copy_amplitudes(state)
        >>> state
        array([1.+0.j, 0.+0.j])

    Apply a Hadamard gate to qubit 0, and then a controlled X gate to qubit 1:

        >>> sim.h(0)
        >>> sim.cx(0, 1)

    Read the amplitudes of the basis states :math:`\\ket{ij0\\dots}`, where :math:`i, j \\in \\{0, 1\\}`:

        >>> state = numpy.zeros(2**2, numpy.complex128)
        >>> sim.copy_amplitudes(state)
        >>> state
        array([0.70710678+0.j, 0.        +0.j, 0.        +0.j, 0.70710678+0.j])

    Measure qubit 1:

        >>> sim.measure(1, 11853927852089066602)
        True

    Read the amplitudes again:

        >>> state = numpy.zeros(2**2, numpy.complex128)
        >>> sim.copy_amplitudes(state)
        >>> state
        array([0.+0.j, 0.+0.j, 0.+0.j, 1.+0.j])
    """

    __slots__ = ()

    def __init__(self, /, **config: typing.Unpack[Config]) -> None:
        pass

    @classmethod
    def max_qubit_count(cls, /) -> int:
        """Return the maxumum supported qubit count.

        Example:

            >>> Simulator.max_qubit_count()
            3968
        """

    def flush(self, /) -> None:
        """Apply all gates in the gate queue.


        Example:

            >>> sim.flush()
        """

    def dump(self, /) -> None:
        """Dump the state vector to stderr."""

    def x(self, target: int, /) -> None:
        """Apply an X gate.

        Equivalent to :code:`sim.u3(target, math.pi, math.pi, 0)`.

        Example:

            >>> sim.x(0)
        """

    def y(self, target: int, /) -> None:
        """Apply a Y gate.

        Equivalent to :code:`sim.u3(target, math.pi, 0, 0)`.

        Example:

            >>> sim.y(0)
        """

    def z(self, target: int, /) -> None:
        """Apply a Z gate.

        Equivalent to :code:`sim.u3(target, 0, math.pi, 0)`.

        Example:

            >>> sim.z(0)
        """

    def h(self, target: int, /) -> None:
        """Apply a Hadamard gate.

        Equivalent to :code:`sim.u3(target, math.pi / 2, 0, math.pi)`.

        Example:

            >>> sim.h(0)
        """

    def s(self, target: int, /) -> None:
        """Apply an S gate.

        Equivalent to :code:`sim.u3(target, 0, math.pi / 2, 0)`.

        Example:

            >>> sim.s(0)
        """

    def sdg(self, target: int, /) -> None:
        """Apply an inverse S gate.

        Equivalent to :code:`sim.u3(target, 0, -math.pi / 2, 0)`.

        Example:

            >>> sim.sdg(0)
        """

    def t(self, target: int, /) -> None:
        """Apply a T gate.

        Equivalent to :code:`sim.u3(target, 0, math.pi / 4, 0)`.

        Example:

            >>> sim.t(0)
        """

    def tdg(self, target: int, /) -> None:
        """Apply an inverse T gate.

        Equivalent to :code:`sim.u3(target, 0, -math.pi / 4, 0).`

        Example:

            >>> sim.tdg(0)
        """

    def rx(self, target: int, theta: float, /) -> None:
        """Apply a rotation about the X axis.

        Equivalent to :code:`sim.u3(target, theta, -math.pi / 2, math.pi / 2)`.

        Example:

            >>> sim.rx(0, math.pi / 2)
        """

    def ry(self, target: int, theta: float, /) -> None:
        """Apply a rotation about the Y axis.

        Equivalent to :code:`sim.u3(target, theta, 0, 0)`.

        Example:

            >>> sim.ry(0, math.pi / 2)
        """

    def rz(self, target: int, theta: float, /) -> None:
        """Apply a rotation about the Z axis.

        Equivalent to :code:`sim.u3(target, 0, theta, 0)`.

        Example:

            >>> sim.rz(0, math.pi / 2)
        """

    def u3(self, target: int, theta: float, phi: float, lam: float, /) -> None:
        """Apply a general single-qubit gate.

        The definition is equivalent up to global phase to the OpenQASM :code:`U` gate:

        .. math::
           U3(\\theta, \\phi, \\lambda) =
           \\left( \\begin{matrix}
           \\cos \\frac{\\theta}{2} & -e^{i \\lambda} \\sin \\frac{\\theta}{2} \\\\
           e^{i \\phi} \\sin \\frac{\\theta}{2} & e^{i(\\phi + \\lambda)} \\cos \\frac{\\theta}{2}
           \\end{matrix} \\right)

        Example:

            >>> sim.u3(0, math.pi / 2, math.pi / 2, math.pi / 2)
        """

    def cx(self, control: int, target: int, /) -> None:
        """Apply a controlled X gate.

        Example:

            >>> sim.cx(0, 1)
        """

    def ccx(self, control1: int, control2: int, target: int, /) -> None:
        """Apply a doubly-controlled X gate.

        Example:

            >>> sim.ccx(0, 1, 2)
        """

    def mcx(self, controls: list[tuple[int, bool]], target: int, /) -> None:
        """Apply a multiply-controlled X gate.

        Each of the :code:`controls` is a pair of :code:`(qubit, value)`.
        The gate is applied to all basis states where the controls have the desired values.

        Example:

            >>> sim.mcx([(0, True), (1, True), (2, True)], 3)
        """

    def swap(self, target1: int, target2: int, /) -> None:
        """Apply a swap gate.

        Example:

            >>> sim.swap(0, 1)
        """

    def cswap(self, control: int, target1: int, target2: int, /) -> None:
        """Apply a controlled swap gate.

        Example:

           >>> sim.cswap(0, 1, 2)
        """

    def mcswap(self, controls: list[tuple[int, bool]], target1: int, target2: int, /) -> None:
        """Apply a multiply-controlled swap gate.

        Each of the :code:`controls` is a pair of :code:`(qubit, value)`.
        The gate is applied to all basis states where the controls have the desired values.

        Example:

            >>> sim.mcswap([(0, True), (1, True), (2, True)], 3, 4)
        """

    def cz(self, control: int, target: int, /) -> None:
        """Apply a controlled Z gate.

        Example:

            >>> sim.cz(0, 1)
        """

    def mcphase(self, controls: list[tuple[int, bool]], theta: float, /) -> None:
        """
        Apply a multi-controlled global phase gate.

        Each of the :code:`controls` is a pair of :code:`(qubit, value)`.
        The gate is applied to all basis states where the controls have the desired values.

        Example: The gate :code:`sim.cz(0, 1)` is equivalent to

            >>> sim.mcphase([(0, True), (1, True)], math.pi)
        """

    def measure(self, target: int, random: int | None = None, /) -> bool:
        """
        Measure a single qubit. Return the outcome.

        The state vector is collapsed.

        The parameter :code:`random` must be a random 64-bit integer (e.g., :code:`random.getrandbits(64)`).

        Example:

            >>> sim.h(0)
            >>> sim.measure(0, 2375645456254334209)
            False
        """

    def measure_ext(self, target: int, random: int | None = None, /) -> tuple[bool, float, float]:
        """
        Similar to :code:`measure`, but also returns the probabilities.

        Example:

            >>> sim.h(0)
            >>> sim.measure_ext(0, 2375645456254334209)
            (False, 0.5, 0.5)
        """

    def qubit_probs(self, target: int, /) -> tuple[float, float]:
        """
        Compute the measurement probabilities for the target qubit without measuring it.

        Example:

            >>> sim.qubit_probs(0)
            (1.0, 0.0)
            >>> sim.h(0)
            >>> sim.qubit_probs(0)
            (0.5, 0.5)
        """

    def copy_amplitudes(self, buffer: collections.abc.Buffer, /) -> None:
        """
        Copy the state vector amplitudes to :code:`buffer`.

        The amplitude of the 'i'th state vector is stored at position 'i',
        where the 'k'th bit of 'i' equals the basis value of the 'k'th qubit.

        The buffer must be one of the following:

        - A single-dimensional array of complex double-precision floating-point numbers.
        - A single-dimensional array of double-precision floating-point numbers of even length.
          Even-indexed elements will store the real parts and odd-indexed elements will store
          the imaginary parts.
        - A C-contiguous two-dimensional array of double-precision floating-point numbers
          with shape :code:`(n, 2)`. Elements :code:`buf[i][0]` will store the real part and
          :code:`buf[i][1]` will store the imaginary part.

        Example:

            >>> for i in range(3): sim.h(i)
            >>> state = numpy.zeros(2**3, numpy.complex128)
            >>> sim.copy_amplitudes(state)
            >>> state
            array([0.35355339+0.j, 0.35355339+0.j, 0.35355339+0.j, 0.35355339+0.j,
                   0.35355339+0.j, 0.35355339+0.j, 0.35355339+0.j, 0.35355339+0.j])
        """

    def __iter__(self, /) -> typing.Iterator[tuple[int, complex]]:
        """
        Iterate over the non-zero state vector amplitudes. The results are returned in an
        unspecified order.

        Example:

            >>> sim.h(0)
            >>> sim.cx(0, 1)
            >>> sim.s(0)
            >>> list(sim)
            [(0, (0.7071067812+0j)), (3, (0+0.7071067812j))]
        """

    def clone(self, /) -> Simulator:
        """
        Clone the quantum state.

        A new independent simulator is returned with a copy of the state of
        this one.

        Example:

            >>> sim.h(0)
            >>> sim2 = sim.clone()
            >>> sim2.measure_ext(0, 15980756942077321109)
            (True, 0.5, 0.5)
            >>> sim.measure_ext(0, 1933827847985506032)
            (False, 0.5, 0.5)
            >>> sim3 = sim.clone()
            >>> sim3.measure_ext(0, 15980756942077321109)
            (False, 1.0, 0.0)
        """

    def _perf(self, /) -> str:
        """
        Return performance data.
        """


class Config(typing.TypedDict, total=False):
    """Simulator configuration options.

    Examples:

    >>> sim = Simulator(dump_config=False)

    >>> sim = Simulator(qubit_count=1)

    >>> sim = Simulator(thread_count=16)

    >>> sim = Simulator(chunk_size=128*2**10)

    >>> sim = Simulator(multithreading_threshold=64*2**10)

    >>> sim = Simulator(work_item_min_size=1*2**10)

    >>> sim = Simulator(work_item_max_size=16*2**20)
    """

    dump_config: bool
    """Dumps the simulator configuration to stderr.

    Default: no.
    """

    qubit_count: int
    """Hint about how many qubits will be used.

    This option determines how many bits are used to represent qubit indices.
    The representation is changed automatically when a qubit with a higher index is used.
    The maximum supported qubit count is returned by by :code:`Simulator.max_qubit_count()`.

    Default: 1.
    """

    thread_count: int
    """The number of threads in the thread pool.

    Default: nproc.
    """

    chunk_size: int
    """The chunk size for processing superposition-free gates (in bytes).

    If set to SIZE_MAX, then a single chunk is used.

    Default: smallest L2 cache size.
    """

    multithreading_threshold: int
    """The state vector size after which multiple threads will be used (in bytes).

    Default: 64 KiB.
    """

    work_item_min_size: int
    """The minimum size of a thread work item (in bytes).

    Use to ensure that the work per item is larger that the communication overhead.

    Default: 1 KiB.
    """

    work_item_max_size: int
    """The maximum size of a thread work item (in bytes).

    Use to ensure that there are enough items for threads that finish early.

    Default: 16 MiB.
    """

    _perf_enabled: bool
    """Enable the collection of performance data.

    Default: False.
    """
