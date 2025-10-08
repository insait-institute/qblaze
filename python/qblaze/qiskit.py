# SPDX-License-Identifier: Apache-2.0
from __future__ import annotations
import array
import collections
import math
import random
import typing
import uuid

import qiskit
import numpy

from . import Simulator, __version__


_zyz_decomposer = qiskit.synthesis.OneQubitEulerDecomposer(basis='ZYZ')
_zxz_decomposer = qiskit.synthesis.OneQubitEulerDecomposer(basis='ZXZ')


# Qiskit 2.x removes Operation.condition
_QISKIT_1: typing.Final[bool] = qiskit.__version__.startswith('1.')


# For testing circuits with high qubit indices.
_QUBIT_INDEX_SCALE: typing.Final[int] = 1


PI: typing.Final[float] = math.pi
MINUS_PI: typing.Final[float] = -PI
TAU: typing.Final[float] = 2 * PI
HALF_PI: typing.Final[float] = PI / 2
MINUS_HALF_PI: typing.Final[float] = -HALF_PI
QUARTER_PI: typing.Final[float] = PI / 4
MINUS_QUARTER_PI: typing.Final[float] = -QUARTER_PI


_T = typing.TypeVar('_T', bound=qiskit.circuit.Operation)
_Handler: typing.TypeAlias = typing.Callable[['_Context', list[tuple[int, bool]], _T, list[qiskit.circuit.Clbit], list[int]], None]


class _Dispatcher(dict[type, _Handler[qiskit.circuit.Operation]]):
    __slots__ = ()

    def __missing__(self, /, ty: type[_T]) -> _Handler[_T]:
        for ty1 in ty.__mro__:
            if ty1 in self:
                r = self[ty] = self[ty1]
                return r
        raise NotImplementedError(ty)

    # FIXME: Not supported, but ideally we'd have
    #   def register[_U](self, ty: type[_T], /) -> typing.Callable[[_Handler[_U]], _Handler[_U]]
    #   where _T: _U
    def register(self, ty: type[_T], /) -> typing.Callable[[_Handler[_T]], _Handler[qiskit.circuit.Operation]]:
        @typing.no_type_check
        def reg(f, /):
            self[ty] = f
            return f
        return reg # type: ignore


class _Context:
    __slots__ = ('rng', 'sv', 'sim', 'clbits', 'force_clbits', 'respect_barriers')
    rng: random.Random
    sv: list[numpy.typing.NDArray[numpy.complex128]]
    sim: Simulator
    clbits: dict[qiskit.circuit.Clbit, bool]
    force_clbits: dict[qiskit.circuit.Clbit, bool]
    respect_barriers: bool
    _CONTROLLED = _Dispatcher()
    _UNCONTROLLED = _Dispatcher()

    def __init__(self, /, rng: random.Random, sv: list[numpy.typing.NDArray[numpy.complex128]], sim: Simulator, force_clbits: dict[qiskit.circuit.Clbit, bool], clbits: dict[qiskit.circuit.Clbit, bool], respect_barriers: bool) -> None:
        self.rng = rng
        self.sv = sv
        self.sim = sim
        self.force_clbits = force_clbits
        self.clbits = clbits
        self.respect_barriers = respect_barriers

    def eval_float(self, e: typing.Any, /) -> float:
        match e:
            case float():
                return e
            case int():
                return float(e)
            case qiskit.circuit.ParameterExpression():
                match (e := e.numeric()):
                    case float():
                        return e
                    case int():
                        return float(e)
        raise TypeError('expected real number')

    def eval_cond(self, cond: tuple[qiskit.ClassicalRegister | qiskit.circuit.Clbit, int] | qiskit.circuit.classical.expr.Expr, /) -> bool:
        match cond:
            case (qiskit.ClassicalRegister() as creg, int() as cval):
                return cval == sum(1 << i for i,bit in enumerate(creg) if self.clbits[bit])
            case (qiskit.circuit.Clbit() as creg, int() as cval):
                return cval == self.clbits[creg]
            case _:
                raise TypeError(f'Bad condition: {cond!r}')

    def subcircuit(self, ctl: list[tuple[int, bool]], qc: qiskit.QuantumCircuit, clbits: list[qiskit.circuit.Clbit], qubits: list[int]) -> None:
        sub_force_clbits = {}
        subclbits = {}

        for (outer_clbit, inner_clbit) in zip(clbits, qc.clbits, strict=True):
            subclbits[inner_clbit] = self.clbits[outer_clbit]
            try:
                val = self.force_clbits[outer_clbit]
            except KeyError:
                pass
            else:
                sub_force_clbits[inner_clbit] = val

        subctx = _Context(self.rng, self.sv, self.sim, sub_force_clbits, subclbits, self.respect_barriers)
        qubit_map = {inner_qubit: outer_qubit for (outer_qubit, inner_qubit) in zip(qubits, qc.qubits, strict=True)}

        disp = self._CONTROLLED if ctl else self._UNCONTROLLED
        for inst in qc.data:
            op = inst.operation
            # Use `_condition` to silence this warning:
            #   DeprecationWarning: The property ``qiskit.circuit.instruction.Instruction.condition`` is deprecated as of qiskit 1.3.0.
            if _QISKIT_1 and isinstance(op, qiskit.circuit.Instruction) and not isinstance(op, qiskit.circuit.ControlFlowOp) and (cond := op._condition) is not None and not subctx.eval_cond(cond):
                continue
            disp[type(op)](subctx, ctl, op, inst.clbits, [qubit_map[q] for q in inst.qubits])

        for (outer_clbit, inner_clbit) in zip(clbits, qc.clbits, strict=True):
            self.clbits[outer_clbit] = subclbits[inner_clbit]

    @_CONTROLLED.register(qiskit.circuit.Delay)
    @_UNCONTROLLED.register(qiskit.circuit.Delay)
    @_UNCONTROLLED.register(qiskit.circuit.library.GlobalPhaseGate)
    @_CONTROLLED.register(qiskit.circuit.library.IGate)
    @_UNCONTROLLED.register(qiskit.circuit.library.IGate)
    def handle_nop(self, ctl: list[tuple[int, bool]], op: qiskit.circuit.Operation, clbits: list[qiskit.circuit.Clbit], qubits: list[int]) -> None:
        pass

    @_CONTROLLED.register(qiskit.circuit.Barrier)
    @_UNCONTROLLED.register(qiskit.circuit.Barrier)
    def handle_barrier(self, ctl: list[tuple[int, bool]], op: qiskit.circuit.Barrier, clbits: list[qiskit.circuit.Clbit], qubits: list[int]) -> None:
        if not self.respect_barriers:
            return
        return self.sim.flush()

    @_CONTROLLED.register(qiskit.circuit.IfElseOp)
    @_UNCONTROLLED.register(qiskit.circuit.IfElseOp)
    def handle_ifelse(self, ctl: list[tuple[int, bool]], op: qiskit.circuit.IfElseOp, clbits: list[qiskit.circuit.Clbit], qubits: list[int]) -> None:
        sub = op.params[not self.eval_cond(op.condition)]
        if sub is None:
            return
        return self.subcircuit(ctl, sub, clbits, qubits)

    @_UNCONTROLLED.register(qiskit.circuit.Measure)
    def handle_measure(self, ctl: list[tuple[int, bool]], op: qiskit.circuit.Measure, clbits: list[qiskit.circuit.Clbit], qubits: list[int]) -> None:
        sim = self.sim
        for (q, clbit) in zip(qubits, clbits, strict=True):
            try:
                want = self.force_clbits[clbit]
            except KeyError:
                want = None
                rnd = self.rng.getrandbits(64)
            else:
                rnd = 2**64-1 if want else 0
            self.clbits[clbit] = got = sim.measure(q, rnd)
            if want is not None:
                assert got == want

    @_UNCONTROLLED.register(qiskit.circuit.Reset)
    def handle_reset(self, ctl: list[tuple[int, bool]], op: qiskit.circuit.Reset, clbits: list[qiskit.circuit.Clbit], qubits: list[int]) -> None:
        assert not clbits
        sim = self.sim
        for q in qubits:
            if sim.measure(q, self.rng.getrandbits(64)):
                sim.x(q)

    @_CONTROLLED.register(qiskit.circuit.ControlledGate)
    @_UNCONTROLLED.register(qiskit.circuit.ControlledGate)
    def handle_ctl(self, ctl: list[tuple[int, bool]], op: qiskit.circuit.ControlledGate, clbits: list[qiskit.circuit.Clbit], qubits: list[int]) -> None:
        ctl_qubits = qubits[:op.num_ctrl_qubits]
        sub_qubits = qubits[op.num_ctrl_qubits:]
        sub_op = op.base_gate
        assert sub_op is not None
        sub_ctl = [
            *ctl,
            *((q, not not (op.ctrl_state & (1 << i))) for (i, q) in enumerate(ctl_qubits)),
        ]
        return self._CONTROLLED[type(sub_op)](self, sub_ctl, sub_op, clbits, sub_qubits)

    @_CONTROLLED.register(qiskit.circuit.library.SwapGate)
    @_UNCONTROLLED.register(qiskit.circuit.library.SwapGate)
    def handle_swap(self, ctl: list[tuple[int, bool]], op: qiskit.circuit.library.SwapGate, clbits: list[qiskit.circuit.Clbit], qubits: list[int]) -> None:
        assert not clbits
        [q0, q1] = qubits
        return self.sim.mcswap(ctl, q0, q1)

    @_CONTROLLED.register(qiskit.circuit.library.CSwapGate)
    @_UNCONTROLLED.register(qiskit.circuit.library.CSwapGate)
    def handle_cswap(self, ctl: list[tuple[int, bool]], op: qiskit.circuit.library.CSwapGate, clbits: list[qiskit.circuit.Clbit], qubits: list[int]) -> None:
        assert not clbits
        [c, q0, q1] = qubits
        return self.sim.mcswap([*ctl, (c, not not (op.ctrl_state & 1))], q0, q1)

    @_UNCONTROLLED.register(qiskit.circuit.library.XGate)
    def handle_uncontrolled_x(self, ctl: list[tuple[int, bool]], op: qiskit.circuit.library.XGate, clbits: list[qiskit.circuit.Clbit], qubits: list[int]) -> None:
        assert not clbits
        [q] = qubits
        return self.sim.x(q)

    @_CONTROLLED.register(qiskit.circuit.library.XGate)
    def handle_x(self, ctl: list[tuple[int, bool]], op: qiskit.circuit.library.XGate, clbits: list[qiskit.circuit.Clbit], qubits: list[int]) -> None:
        assert not clbits
        [q] = qubits
        return self.sim.mcx(ctl, q)

    @_CONTROLLED.register(qiskit.circuit.library.CXGate)
    @_UNCONTROLLED.register(qiskit.circuit.library.CXGate)
    def handle_cx(self, ctl: list[tuple[int, bool]], op: qiskit.circuit.library.CXGate, clbits: list[qiskit.circuit.Clbit], qubits: list[int]) -> None:
        assert not clbits
        [c, q] = qubits
        return self.sim.mcx([*ctl, (c, not not (op.ctrl_state & 1))], q)

    @_CONTROLLED.register(qiskit.circuit.library.CCXGate)
    @_UNCONTROLLED.register(qiskit.circuit.library.CCXGate)
    def handle_ccx(self, ctl: list[tuple[int, bool]], op: qiskit.circuit.library.CCXGate, clbits: list[qiskit.circuit.Clbit], qubits: list[int]) -> None:
        assert not clbits
        [c1, c2, q] = qubits
        return self.sim.mcx([*ctl, (c1, not not (op.ctrl_state & 1)), (c2, not not (op.ctrl_state & 2))], q)

    @_CONTROLLED.register(qiskit.circuit.library.C3XGate)
    @_UNCONTROLLED.register(qiskit.circuit.library.C3XGate)
    def handle_c3x(self, ctl: list[tuple[int, bool]], op: qiskit.circuit.library.C3XGate, clbits: list[qiskit.circuit.Clbit], qubits: list[int]) -> None:
        assert not clbits
        [c1, c2, c3, q] = qubits
        return self.sim.mcx([*ctl, (c1, not not (op.ctrl_state & 1)), (c2, not not (op.ctrl_state & 2)), (c3, not not (op.ctrl_state & 4))], q)

    @_CONTROLLED.register(qiskit.circuit.library.C4XGate)
    @_UNCONTROLLED.register(qiskit.circuit.library.C4XGate)
    def handle_c4x(self, ctl: list[tuple[int, bool]], op: qiskit.circuit.library.C4XGate, clbits: list[qiskit.circuit.Clbit], qubits: list[int]) -> None:
        assert not clbits
        [c1, c2, c3, c4, q] = qubits
        return self.sim.mcx([*ctl, (c1, not not (op.ctrl_state & 1)), (c2, not not (op.ctrl_state & 2)), (c3, not not (op.ctrl_state & 4)), (c4, not not (op.ctrl_state & 8))], q)

    @_UNCONTROLLED.register(qiskit.circuit.library.ZGate)
    def handle_uncontrolled_z(self, ctl: list[tuple[int, bool]], op: qiskit.circuit.library.ZGate, clbits: list[qiskit.circuit.Clbit], qubits: list[int]) -> None:
        assert not clbits
        [q] = qubits
        return self.sim.z(q)

    @_CONTROLLED.register(qiskit.circuit.library.ZGate)
    def handle_z(self, ctl: list[tuple[int, bool]], op: qiskit.circuit.library.ZGate, clbits: list[qiskit.circuit.Clbit], qubits: list[int]) -> None:
        assert not clbits
        [q] = qubits
        return self.sim.mcphase([*ctl, (q, True)], PI)

    @_UNCONTROLLED.register(qiskit.circuit.library.SGate)
    def handle_uncontrolled_s(self, ctl: list[tuple[int, bool]], op: qiskit.circuit.library.SGate, clbits: list[qiskit.circuit.Clbit], qubits: list[int]) -> None:
        assert not clbits
        [q] = qubits
        return self.sim.s(q)

    @_CONTROLLED.register(qiskit.circuit.library.SGate)
    def handle_s(self, ctl: list[tuple[int, bool]], op: qiskit.circuit.library.SGate, clbits: list[qiskit.circuit.Clbit], qubits: list[int]) -> None:
        assert not clbits
        [q] = qubits
        return self.sim.mcphase([*ctl, (q, True)], HALF_PI)

    @_UNCONTROLLED.register(qiskit.circuit.library.SdgGate)
    def handle_uncontrolled_sdg(self, ctl: list[tuple[int, bool]], op: qiskit.circuit.library.SdgGate, clbits: list[qiskit.circuit.Clbit], qubits: list[int]) -> None:
        assert not clbits
        [q] = qubits
        return self.sim.sdg(q)

    @_CONTROLLED.register(qiskit.circuit.library.SdgGate)
    def handle_sdg(self, ctl: list[tuple[int, bool]], op: qiskit.circuit.library.SdgGate, clbits: list[qiskit.circuit.Clbit], qubits: list[int]) -> None:
        assert not clbits
        [q] = qubits
        return self.sim.mcphase([*ctl, (q, True)], MINUS_HALF_PI)

    @_UNCONTROLLED.register(qiskit.circuit.library.TGate)
    def handle_uncontrolled_t(self, ctl: list[tuple[int, bool]], op: qiskit.circuit.library.TGate, clbits: list[qiskit.circuit.Clbit], qubits: list[int]) -> None:
        assert not clbits
        [q] = qubits
        return self.sim.t(q)

    @_CONTROLLED.register(qiskit.circuit.library.TGate)
    def handle_t(self, ctl: list[tuple[int, bool]], op: qiskit.circuit.library.TGate, clbits: list[qiskit.circuit.Clbit], qubits: list[int]) -> None:
        assert not clbits
        [q] = qubits
        return self.sim.mcphase([*ctl, (q, True)], QUARTER_PI)

    @_UNCONTROLLED.register(qiskit.circuit.library.TdgGate)
    def handle_uncontrolled_tdg(self, ctl: list[tuple[int, bool]], op: qiskit.circuit.library.TdgGate, clbits: list[qiskit.circuit.Clbit], qubits: list[int]) -> None:
        assert not clbits
        [q] = qubits
        return self.sim.tdg(q)

    @_CONTROLLED.register(qiskit.circuit.library.TdgGate)
    def handle_tdg(self, ctl: list[tuple[int, bool]], op: qiskit.circuit.library.TdgGate, clbits: list[qiskit.circuit.Clbit], qubits: list[int]) -> None:
        assert not clbits
        [q] = qubits
        return self.sim.mcphase([*ctl, (q, True)], MINUS_QUARTER_PI)

    @_CONTROLLED.register(qiskit.circuit.library.CZGate)
    @_UNCONTROLLED.register(qiskit.circuit.library.CZGate)
    def handle_cz(self, ctl: list[tuple[int, bool]], op: qiskit.circuit.library.CZGate, clbits: list[qiskit.circuit.Clbit], qubits: list[int]) -> None:
        assert not clbits
        [q0, q1] = qubits
        return self.sim.mcphase([*ctl, (q0, not not (op.ctrl_state & 1)), (q1, True)], PI)

    @_CONTROLLED.register(qiskit.circuit.library.CSGate)
    @_UNCONTROLLED.register(qiskit.circuit.library.CSGate)
    def handle_cs(self, ctl: list[tuple[int, bool]], op: qiskit.circuit.library.CSGate, clbits: list[qiskit.circuit.Clbit], qubits: list[int]) -> None:
        assert not clbits
        [q0, q1] = qubits
        return self.sim.mcphase([*ctl, (q0, not not (op.ctrl_state & 1)), (q1, True)], HALF_PI)

    @_CONTROLLED.register(qiskit.circuit.library.CSdgGate)
    @_UNCONTROLLED.register(qiskit.circuit.library.CSdgGate)
    def handle_csdg(self, ctl: list[tuple[int, bool]], op: qiskit.circuit.library.CSdgGate, clbits: list[qiskit.circuit.Clbit], qubits: list[int]) -> None:
        assert not clbits
        [q0, q1] = qubits
        return self.sim.mcphase([*ctl, (q0, not not (op.ctrl_state & 1)), (q1, True)], MINUS_HALF_PI)

    @_CONTROLLED.register(qiskit.circuit.library.CCZGate)
    @_UNCONTROLLED.register(qiskit.circuit.library.CCZGate)
    def handle_ccz(self, ctl: list[tuple[int, bool]], op: qiskit.circuit.library.CCZGate, clbits: list[qiskit.circuit.Clbit], qubits: list[int]) -> None:
        assert not clbits
        [q0, q1, q2] = qubits
        return self.sim.mcphase([*ctl, (q0, not not (op.ctrl_state & 1)), (q1, not not (op.ctrl_state & 2)), (q2, True)], PI)

    @_CONTROLLED.register(qiskit.circuit.library.GlobalPhaseGate)
    def handle_gphase(self, ctl: list[tuple[int, bool]], op: qiskit.circuit.library.GlobalPhaseGate, clbits: list[qiskit.circuit.Clbit], qubits: list[int]) -> None:
        assert not clbits
        assert not qubits
        [phase] = op.params
        return self.sim.mcphase(ctl, self.eval_float(phase))

    @_UNCONTROLLED.register(qiskit.circuit.library.U1Gate)
    @_UNCONTROLLED.register(qiskit.circuit.library.PhaseGate)
    @_UNCONTROLLED.register(qiskit.circuit.library.RZGate)
    def handle_uncontrolled_rz(self, ctl: list[tuple[int, bool]], op: qiskit.circuit.Gate, clbits: list[qiskit.circuit.Clbit], qubits: list[int]) -> None:
        assert not clbits
        [q] = qubits
        [phase] = op.params
        return self.sim.rz(q, self.eval_float(phase))

    @_CONTROLLED.register(qiskit.circuit.library.U1Gate)
    @_CONTROLLED.register(qiskit.circuit.library.PhaseGate)
    def handle_u1(self, ctl: list[tuple[int, bool]], op: qiskit.circuit.Gate, clbits: list[qiskit.circuit.Clbit], qubits: list[int]) -> None:
        assert not clbits
        [q] = qubits
        [phase] = op.params
        return self.sim.mcphase([*ctl, (q, True)], self.eval_float(phase))

    @_CONTROLLED.register(qiskit.circuit.library.RZGate)
    def handle_rz(self, ctl: list[tuple[int, bool]], op: qiskit.circuit.library.RZGate, clbits: list[qiskit.circuit.Clbit], qubits: list[int]) -> None:
        assert not clbits
        [q] = qubits
        [phase] = op.params
        phi = self.eval_float(phase)
        self.sim.mcphase(ctl, -phi / 2)
        return self.sim.mcphase([*ctl, (q, True)], phi)

    @_CONTROLLED.register(qiskit.circuit.library.CU1Gate)
    @_UNCONTROLLED.register(qiskit.circuit.library.CU1Gate)
    @_CONTROLLED.register(qiskit.circuit.library.CPhaseGate)
    @_UNCONTROLLED.register(qiskit.circuit.library.CPhaseGate)
    def handle_cu1(self, ctl: list[tuple[int, bool]], op: qiskit.circuit.Gate, clbits: list[qiskit.circuit.Clbit], qubits: list[int]) -> None:
        assert not clbits
        [q0, q1] = qubits
        [phase] = op.params
        return self.sim.mcphase([*ctl, (q0, not not (op.ctrl_state & 1)), (q1, True)], self.eval_float(phase))

    @_UNCONTROLLED.register(qiskit.circuit.library.RXGate)
    def handle_rx(self, ctl: list[tuple[int, bool]], op: qiskit.circuit.library.RXGate, clbits: list[qiskit.circuit.Clbit], qubits: list[int]) -> None:
        assert not clbits
        [q] = qubits
        [phase] = op.params
        return self.sim.rx(q, self.eval_float(phase))

    @_UNCONTROLLED.register(qiskit.circuit.library.SXGate)
    def handle_sx(self, ctl: list[tuple[int, bool]], op: qiskit.circuit.library.SXGate, clbits: list[qiskit.circuit.Clbit], qubits: list[int]) -> None:
        assert not clbits
        [q] = qubits
        return self.sim.rx(q, HALF_PI)

    @_UNCONTROLLED.register(qiskit.circuit.library.SXdgGate)
    def handle_sxdg(self, ctl: list[tuple[int, bool]], op: qiskit.circuit.library.SXdgGate, clbits: list[qiskit.circuit.Clbit], qubits: list[int]) -> None:
        assert not clbits
        [q] = qubits
        return self.sim.rx(q, MINUS_HALF_PI)

    @_UNCONTROLLED.register(qiskit.circuit.library.RYGate)
    def handle_ry(self, ctl: list[tuple[int, bool]], op: qiskit.circuit.library.RYGate, clbits: list[qiskit.circuit.Clbit], qubits: list[int]) -> None:
        assert not clbits
        [q] = qubits
        [phase] = op.params
        return self.sim.ry(q, self.eval_float(phase))

    @_UNCONTROLLED.register(qiskit.circuit.library.UGate)
    @_UNCONTROLLED.register(qiskit.circuit.library.U3Gate)
    def handle_u3(self, ctl: list[tuple[int, bool]], op: qiskit.circuit.Gate, clbits: list[qiskit.circuit.Clbit], qubits: list[int]) -> None:
        assert not clbits
        [q] = qubits
        [theta, phi, lam] = op.params
        return self.sim.u3(q, self.eval_float(theta), self.eval_float(phi), self.eval_float(lam))

    @_UNCONTROLLED.register(qiskit.circuit.library.U2Gate)
    def handle_u2(self, ctl: list[tuple[int, bool]], op: qiskit.circuit.library.U2Gate, clbits: list[qiskit.circuit.Clbit], qubits: list[int]) -> None:
        assert not clbits
        [q] = qubits
        [phi, lam] = op.params
        return self.sim.u3(q, HALF_PI, self.eval_float(phi), self.eval_float(lam))

    @_UNCONTROLLED.register(qiskit.circuit.library.HGate)
    def handle_h(self, ctl: list[tuple[int, bool]], op: qiskit.circuit.library.HGate, clbits: list[qiskit.circuit.Clbit], qubits: list[int]) -> None:
        assert not clbits
        [q] = qubits
        return self.sim.h(q)

    @_UNCONTROLLED.register(qiskit.circuit.Instruction)
    def handle_generic_uncontrolled(self, ctl: list[tuple[int, bool]], op: qiskit.circuit.Instruction, clbits: list[qiskit.circuit.Clbit], qubits: list[int]) -> None:
        if type(op).__name__ == 'SaveStatevector' and type(op).__module__ == 'qiskit_aer.library.save_instructions.save_statevector':
            # pershot: bool = op._subtype == 'list'
            # if not pershot:
            #     self.sv.clear()
            nq = max(qubits) // _QUBIT_INDEX_SCALE + 1
            sv = numpy.zeros(2**nq, numpy.complex128)
            if _QUBIT_INDEX_SCALE > 1:
                for i in range(1, nq):
                    self.sim.swap(i, _QUBIT_INDEX_SCALE * i)
            self.sim.copy_amplitudes(sv)
            if _QUBIT_INDEX_SCALE > 1:
                for i in reversed(range(1, nq)):
                    self.sim.swap(i, _QUBIT_INDEX_SCALE * i)
            self.sv.append(sv)
            return

        match op:
            case qiskit.circuit.Gate() if op.num_qubits == 1:
                assert not clbits
                [q] = qubits
                (theta, phi, lam) = _zyz_decomposer.angles(op.to_matrix())
                return self.sim.u3(q, theta, phi, lam)

        if (sub := op.definition) is not None:
            return self.subcircuit(ctl, sub, clbits, qubits)
        raise NotImplementedError(op)

    @_CONTROLLED.register(qiskit.circuit.Instruction)
    def handle_generic_controlled(self, ctl: list[tuple[int, bool]], op: qiskit.circuit.Instruction, clbits: list[qiskit.circuit.Clbit], qubits: list[int]) -> None:
        match op:
            case qiskit.circuit.Gate() if op.num_qubits == 1:
                assert not clbits
                [q] = qubits
                subctl = [*ctl, (q, True)]
                # rz(lam); h; rz(theta); h; rz(phi)
                (theta, phi, lam, corr) = _zxz_decomposer.angles_and_phase(op.to_matrix())
                corr -= (theta + phi + lam) / 2
                sim = self.sim
                if abs(theta) < 1e-8:
                    sim.mcphase(subctl, lam + phi)
                elif abs(theta - PI) < 1e-8 or abs(theta + PI) < 1e-8:
                    sim.mcphase(subctl, lam - phi)
                    sim.mcx(ctl, q)
                else:
                    if abs(lam) >= 1e-8:
                        sim.mcphase(subctl, lam)
                    sim.h(q)
                    sim.mcphase(subctl, theta)
                    sim.h(q)
                    if abs(phi) >= 1e-8:
                        sim.mcphase(subctl, phi)
                while corr > PI:
                    corr -= TAU
                while corr <= MINUS_PI:
                    corr += TAU
                if abs(corr) >= 1e-8:
                    sim.mcphase(ctl, corr)
                return

        if (sub := op.definition) is not None:
            return self.subcircuit(ctl, sub, clbits, qubits)
        raise NotImplementedError(op)


def run_circuit(
    sim: Simulator, qc: qiskit.QuantumCircuit,
    /,
    rng: random.Random | None = None,
    *,
    force_clbits: dict[qiskit.circuit.Clbit, bool] = {},
    respect_barriers: bool = False,
) -> tuple[dict[qiskit.circuit.Clbit, bool], list[numpy.typing.NDArray[numpy.complex128]]]:
    """Run a circuit on the given simulator.

    :code:`force_clbits` can be used to force the outcomes of measurements
    associated with particular classical bits. Simulation will fail if a force
    outcome has zero (or very close to zero) probability.

    Returns :code:`(clbits, statevectors)`, where :code:`clbits` is the value of
    all classical bits at the end of the simulation, and :code:`statevectors` is
    the list of all statevectors at :code:`save_statevector` instructions.

    Example:

    >>> import qiskit
    >>> import qiskit_aer
    >>> from qblaze.qiskit import run_circuit
    >>>
    >>> def make_circuit():
    ...     circ = qiskit.QuantumCircuit(2, 1)
    ...     circ.h(0)
    ...     circ.save_statevector()
    ...     circ.cx(0, 1)
    ...     circ.measure(1, 0)
    ...     circ.save_statevector()
    ...     return circ
    >>>
    >>> circ = make_circuit()
    >>> sim = Simulator()
    >>> rng = random.Random(42)
    >>>
    >>> (clbits, [sv1, sv2]) = run_circuit(sim, circ, rng=rng)
    >>>
    >>> [complex(v) for v in sv1]
    [(0.7071067812+0j), (0.7071067812+0j), 0j, 0j]
    >>> clbits[circ.clbits[0]]
    False
    >>> [complex(v) for v in sv2]
    [(1+0j), 0j, 0j, 0j]
    """

    if rng is None:
        rng = random.Random()

    sv: list[numpy.typing.NDArray[numpy.complex128]] = []
    clbits = {c: False for c in qc.clbits}
    ctx = _Context(rng, sv, sim, force_clbits, clbits, respect_barriers)

    qubit_map = {q: _QUBIT_INDEX_SCALE * i for i, q in enumerate(qc.qubits)}

    ctl: list[tuple[int, bool]] = []
    disp = ctx._UNCONTROLLED
    for inst in qc.data:
        op = inst.operation
        # Use `_condition` to silence this warning:
        #   DeprecationWarning: The property ``qiskit.circuit.instruction.Instruction.condition`` is deprecated as of qiskit 1.3.0.
        if _QISKIT_1 and isinstance(op, qiskit.circuit.Instruction) and not isinstance(op, qiskit.circuit.ControlFlowOp) and (cond := op._condition) is not None and not ctx.eval_cond(cond):
            continue
        disp[type(op)](ctx, ctl, op, inst.clbits, [qubit_map[q] for q in inst.qubits])

    return (clbits, sv)


class Backend(qiskit.providers.BackendV2):
    """Implements :code:`qiskit.providers.BackendV2`."""
    provider: object
    __target: typing.ClassVar[qiskit.transpiler.Target | None] = None

    def __init__(self, /, provider: typing.Any = None) -> None:
        super().__init__(
            provider = provider,
            name = __name__[:__name__.index('.')],
            description = 'Sparse statevector simulator',
            backend_version = __version__,
        )

    @property
    def max_circuits(self, /) -> int:
        """Returns 1."""
        return 1

    @property
    def target(self, /) -> qiskit.transpiler.Target:
        """Returns a :code:`qiskit.transpiler.Target` corresponding to the gates
        natively supported by qblaze."""
        cls = type(self)
        if (target := cls.__target) is not None:
            return target
        target = qiskit.transpiler.Target.from_configuration(
            basis_gates = ['ccx', 'ccz', 'cp', 'crz', 'cs', 'csdg', 'cswap', 'cu1', 'cx', 'cy', 'cz', 'h', 'id', 'measure', 'p', 'reset', 'rx', 'ry', 'rz', 's', 'sdg', 'swap', 'sx', 'sxdg', 't', 'tdg', 'u', 'u1', 'u2', 'u3', 'x', 'y', 'z'],
            num_qubits = Simulator.max_qubit_count(),
        )
        cls.__target = target
        return target

    @classmethod
    def _default_options(cls, /) -> qiskit.providers.Options:
        return qiskit.providers.Options(shots=1, seed_simulator=None)

    def run(self, /, run_input: typing.Any, **options: typing.Any) -> Job:
        """Simulates the given circuit."""
        quantum_circuits: list[qiskit.QuantumCircuit]
        if isinstance(run_input, qiskit.QuantumCircuit):
            quantum_circuits = [run_input]
        else:
            quantum_circuits = [*run_input]

        options = {**self.options, **options}
        job = Job(self, quantum_circuits, options)
        job.submit()
        return job


class Job(qiskit.providers.JobV1):
    """Implements :code:`qiskit.providers.JobV1`."""
    __circuits: list[qiskit.QuantumCircuit]
    __options: dict[str, typing.Any]
    __result: None | list[qiskit.result.models.ExperimentResult]
    __qobj_id: str

    def __init__(self, backend: Backend, quantum_circuits: list[qiskit.QuantumCircuit], options: dict[str, typing.Any]) -> None:
        super().__init__(backend=backend, job_id=f'{uuid.uuid4()!s}')
        self.__circuits = quantum_circuits
        self.__options = options
        self.__qobj_id = f'{uuid.uuid4()!s}'

    def status(self, /) -> qiskit.providers.JobStatus:
        """Returns the status of the job."""
        try:
            res = self.__result
        except AttributeError:
            raise qiskit.providers.JobError('Job not submitted')
        if res is None:
            return qiskit.providers.JobStatus.RUNNING
        return qiskit.providers.JobStatus.DONE

    def result(self, /) -> qiskit.result.Result:
        """Returns the result of the job."""
        try:
            res = self.__result
        except AttributeError:
            raise qiskit.providers.JobError('Job not submitted')
        if res is None:
            raise qiskit.providers.JobError('Job still running')
        return qiskit.result.Result(
            backend_name = self.backend().name,
            backend_version = self.backend().backend_version,
            job_id = self.job_id(),
            qobj_id = self.__qobj_id,
            success = True,
            results = res,
        )

    def submit(self, /) -> None:
        """Runs the job."""
        try:
            self.__result
        except AttributeError:
            pass
        else:
            raise qiskit.providers.JobError('Job already submitted')
        self.__result = None

        seed: int | None = self.__options.get('seed_simulator')
        shots: int | None = self.__options.get('shots')
        if shots is None:
            shots = 1

        run_options = {
            opt: val
            for opt, val in self.__options.items()
            if opt in {'force_clbits', 'respect_barriers'}
        }
        sim_options = {
            opt: val
            for opt, val in self.__options.items()
            if opt not in {'seed_simulator', 'shots', 'force_clbits', 'respect_barriers'}
        }

        res = []
        for qc in self.__circuits:
            counts: dict[str, int] = {}
            sv: list[numpy.typing.NDArray[numpy.complex128]] = []
            for shot in range(shots):
                sim = Simulator(**sim_options)
                run_clbits, run_sv = run_circuit(
                    sim, qc,
                    rng = random.Random(seed + shot) if seed is not None else None,
                    **run_options,
                )
                sv.extend(run_sv)
                m = sum(1 << i for i, clbit in enumerate(qc.clbits) if run_clbits[clbit])
                key = f'{m:#x}'
                counts[key] = counts.get(key, 0) + 1
            res.append(qiskit.result.models.ExperimentResult(
                shots = shots,
                success = True,
                data = qiskit.result.models.ExperimentResultData(
                    counts = counts,
                    statevector = sv,
                ),
                seed = self.__options.get('seed_simulator'),
            ))
        self.__result = res
