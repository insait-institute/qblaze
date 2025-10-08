import pytest

import warnings
import random
import math

import qblaze.qiskit
import qiskit
import numpy


TAU = 2 * math.pi


def clear_gphase(*arrays):
    pos = numpy.unravel_index(numpy.argmax(numpy.abs(arrays[0])), arrays[0].shape)
    return [arr / (arr[pos] / numpy.abs(arr[pos])) for arr in arrays]


# (gate type, number of parameters)
GATES = [
    (qiskit.circuit.library.IGate, 0),
    (qiskit.circuit.library.XGate, 0),
    (qiskit.circuit.library.YGate, 0),
    (qiskit.circuit.library.ZGate, 0),
    (qiskit.circuit.library.HGate, 0),
    (qiskit.circuit.library.CXGate, 0),
    (qiskit.circuit.library.CYGate, 0),
    (qiskit.circuit.library.CZGate, 0),
    (qiskit.circuit.library.CCXGate, 0),
    (qiskit.circuit.library.C3XGate, 0),
    (qiskit.circuit.library.CCZGate, 0),
    (qiskit.circuit.library.RCCXGate, 0),
    (qiskit.circuit.library.RC3XGate, 0),
    (qiskit.circuit.library.CHGate, 0),
    (qiskit.circuit.library.SGate, 0),
    (qiskit.circuit.library.SdgGate, 0),
    (qiskit.circuit.library.CSGate, 0),
    (qiskit.circuit.library.CSdgGate, 0),
    (qiskit.circuit.library.TGate, 0),
    (qiskit.circuit.library.TdgGate, 0),
    (qiskit.circuit.library.SXGate, 0),
    (qiskit.circuit.library.SXdgGate, 0),
    (qiskit.circuit.library.CSXGate, 0),
    (qiskit.circuit.library.C3SXGate, 0),
    (qiskit.circuit.library.SwapGate, 0),
    (qiskit.circuit.library.CSwapGate, 0),
    (qiskit.circuit.library.iSwapGate, 0),
    (qiskit.circuit.library.GlobalPhaseGate, 1),
    (qiskit.circuit.library.RXXGate, 1),
    (qiskit.circuit.library.RYYGate, 1),
    (qiskit.circuit.library.RZZGate, 1),
    (qiskit.circuit.library.RZXGate, 1),
    (qiskit.circuit.library.RVGate, 3),
    (qiskit.circuit.library.RXGate, 1),
    (qiskit.circuit.library.CRXGate, 1),
    (qiskit.circuit.library.RYGate, 1),
    (qiskit.circuit.library.CRYGate, 1),
    (qiskit.circuit.library.RZGate, 1),
    (qiskit.circuit.library.CRZGate, 1),
    (qiskit.circuit.library.RGate, 2),
    (qiskit.circuit.library.PhaseGate, 1),
    (qiskit.circuit.library.CPhaseGate, 1),
    (qiskit.circuit.library.U1Gate, 1),
    (qiskit.circuit.library.CU1Gate, 1),
    (qiskit.circuit.library.U2Gate, 2),
    (qiskit.circuit.library.UGate, 3),
    # (qiskit.circuit.library.CUGate, 4), # to_matrix() crashes for the controlled version; also broken when the last parameter is non-zero?
    (qiskit.circuit.library.U3Gate, 3),
    (qiskit.circuit.library.CU3Gate, 3),
]


@pytest.mark.parametrize("gate_type, n_params", GATES)
def test_gate(gate_type, n_params):
    gate = gate_type(*(random.random() * TAU for _ in range(n_params)))

    n = gate.num_qubits
    q1 = qiskit.QuantumRegister(n)
    q2 = qiskit.QuantumRegister(n)
    circ = qiskit.QuantumCircuit(q1, q2)
    if n:
        circ.h(q1)
    for q1i, q2i in zip(q1, q2, strict=True):
        circ.cx(q1i, q2i)
    circ.append(gate, [*q2])

    sim = qblaze.Simulator()
    qblaze.qiskit.run_circuit(sim, circ)
    sv = numpy.zeros(2**(2*n), numpy.complex128)
    sim.flush()
    sim.copy_amplitudes(sv)

    m_got = sv.reshape((2**n, 2**n)) * 2**(n/2)
    m_want = gate.to_matrix()
    (m_got, m_want) = clear_gphase(m_got, m_want)

    if numpy.abs(m_got - m_want).max() > 0.001:
        raise RuntimeError(f'Gate error for {gate}:\ngot:\n{m_got}\nwant:\n{m_want}')


@pytest.mark.parametrize("gate_type, n_params", GATES)
@pytest.mark.parametrize("n_controls, mask", [(1, 0), (1, 1), (2, 0), (2, 1), (2, 2), (2, 3)])
def test_controlled_gate(gate_type, n_params, n_controls, mask):
    gate = gate_type(*(random.random() * TAU for _ in range(n_params)))

    n = gate.num_qubits
    qc = qiskit.QuantumRegister(n_controls)
    q1 = qiskit.QuantumRegister(n)
    q2 = qiskit.QuantumRegister(n)
    circ = qiskit.QuantumCircuit(q1, q2, qc)
    circ.h(qc)
    if n:
        circ.h(q1)
    for q1i, q2i in zip(q1, q2, strict=True):
        circ.cx(q1i, q2i)

    with warnings.catch_warnings():
        warnings.simplefilter('ignore', category=(DeprecationWarning, PendingDeprecationWarning))
        # Silence the following warnings:
        #   DeprecationWarning: The property ``qiskit.dagcircuit.dagcircuit.DAGCircuit.duration`` is deprecated as of Qiskit 1.3.0.
        #   DeprecationWarning: The property ``qiskit.dagcircuit.dagcircuit.DAGCircuit.unit`` is deprecated as of qiskit 1.3.0.
        #   PendingDeprecationWarning: The method ``qiskit.circuit.library.standard_gates.x.MCXGate.get_num_ancilla_qubits()`` is pending deprecation as of qiskit 1.3.
        cgate = gate.control(n_controls, ctrl_state=mask)
    circ.append(cgate, [*qc, *q2])

    sim = qblaze.Simulator()
    qblaze.qiskit.run_circuit(sim, circ)
    sv = numpy.zeros(2**(n_controls + 2*n), numpy.complex128)
    sim.flush()
    sim.copy_amplitudes(sv)

    m_got = sv.reshape((2**n_controls, 2**n, 2**n)) * 2**((n+n_controls)/2)
    for i in range(2**n_controls):
        if i == mask:
            continue
        (m_got0,) = clear_gphase(m_got[i])
        if numpy.abs(m_got0 - numpy.eye(2**n)).max() > 0.001:
            raise RuntimeError(f'Gate error for controlled {gate} when disabled:\ngot:\n{m_got0}\nwant: identity')

    (m_got1, m_want) = clear_gphase(m_got[mask], gate.to_matrix())
    if numpy.abs(m_got1 - m_want).max() > 0.001:
        raise RuntimeError(f'Gate error for controlled {gate} when enabled:\ngot:\n{m_got1}\nwant:\n{m_want}')



@pytest.mark.skipif(not qiskit.__version__.startswith('1.'), reason='c_if removed in Qiskit 2.0')
def test_condition():
    c = qiskit.ClassicalRegister(1)
    q = qiskit.QuantumRegister(3)
    circ = qiskit.QuantumCircuit(c, q)

    with warnings.catch_warnings():
        warnings.simplefilter('ignore', category=DeprecationWarning)
        circ.append(qiskit.circuit.library.XGate(), [q[0]])
        circ.append(qiskit.circuit.library.XGate().c_if(c[0], True), [q[1]])
        circ.append(qiskit.circuit.library.XGate().c_if(c[0], False), [q[2]])

    sim = qblaze.Simulator()
    qblaze.qiskit.run_circuit(sim, circ)
    sv = numpy.zeros(2**3, numpy.complex128)
    sim.flush()
    sim.copy_amplitudes(sv)

    assert numpy.abs(sv).argmax() == 5


def test_ifelse():
    c = qiskit.ClassicalRegister(1)
    q = qiskit.QuantumRegister(2)

    circ0 = qiskit.QuantumCircuit(q)
    circ0.x(q[0])
    circ1 = qiskit.QuantumCircuit(q)
    circ1.x(q[1])

    q0 = qiskit.QuantumRegister(2)
    q1 = qiskit.QuantumRegister(2)
    circ = qiskit.QuantumCircuit(c, q0, q1)
    circ.append(qiskit.circuit.IfElseOp((c, 0), circ0, circ1), q0)
    circ.append(qiskit.circuit.IfElseOp((c, 1), circ0, circ1), q1)

    sim = qblaze.Simulator()
    qblaze.qiskit.run_circuit(sim, circ)
    sv = numpy.zeros(2**4, numpy.complex128)
    sim.flush()
    sim.copy_amplitudes(sv)

    assert numpy.abs(sv).argmax() == 9
