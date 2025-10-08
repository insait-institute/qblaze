#!/usr/bin/python3
# SPDX-License-Identifier: Apache-2.0
import sys
import random
import numpy
import time
import traceback

import qiskit
import qiskit.qasm2
import qiskit_aer

import qblaze.qiskit


SINGLE_QUBIT_GATES = (
    qiskit.circuit.library.HGate(),
    qiskit.circuit.library.HGate(),
    qiskit.circuit.library.SXGate(),
    qiskit.circuit.library.SXdgGate(),
    qiskit.circuit.library.XGate(),
    qiskit.circuit.library.ZGate(),
    qiskit.circuit.library.SGate(),
    qiskit.circuit.library.SdgGate(),
    qiskit.circuit.library.TGate(),
    qiskit.circuit.library.TdgGate(),
)

TWO_QUBIT_GATES = (
    qiskit.circuit.library.CXGate(),
    qiskit.circuit.library.XGate().control(1, ctrl_state=0),
    qiskit.circuit.library.CZGate(),
    qiskit.circuit.library.ZGate().control(1, ctrl_state=0),
    qiskit.circuit.library.SwapGate(),
)

THREE_QUBIT_GATES = (
    qiskit.circuit.library.CCXGate(),
    qiskit.circuit.library.XGate().control(2, ctrl_state=0),
    qiskit.circuit.library.XGate().control(2, ctrl_state=1),
    qiskit.circuit.library.XGate().control(2, ctrl_state=2),
    qiskit.circuit.library.CCZGate(),
    qiskit.circuit.library.ZGate().control(2, ctrl_state=0),
    qiskit.circuit.library.ZGate().control(2, ctrl_state=1),
    qiskit.circuit.library.ZGate().control(2, ctrl_state=2),
    qiskit.circuit.library.CSwapGate(),
    # qiskit.circuit.library.SwapGate().control(1, ctrl_state=0), # broken in qiskit-aer
)


def test(circuit: qiskit.QuantumCircuit) -> bool:
    n_qubits = len(circuit.qubits)
    circuit.save_statevector(pershot=True)

    sim1 = qiskit_aer.AerSimulator(method='statevector', precision='double', device='CPU', fusion_enable=False)
    sim2 = qblaze.qiskit.Backend(None)

    t1 = time.monotonic_ns()
    res1 = sim1.run(circuit, shots=1).result()
    t1 = time.monotonic_ns() - t1

    if circuit.num_clbits > 0:
        [got] = res1.get_counts().keys()
        got = ''.join(got.split(' '))[::-1]
        got_clbits = {clbit: got[i] == '1' for i, clbit in enumerate(circuit.clbits)}
    else:
        got_clbits = {}

    t2 = time.monotonic_ns()
    res2 = sim2.run(circuit, shots=1, respect_barriers=True, force_clbits=got_clbits).result()
    t2 = time.monotonic_ns() - t2

    [sv1] = res1.data()['statevector']
    [sv2] = res2.data()['statevector']
    sv1 = numpy.asarray(sv1)
    assert len(sv1) == 2**n_qubits
    assert len(sv2) == 2**n_qubits

    eps = 2**-24

    maxi = 0
    maxv = 0.0
    for i in range(2**n_qubits):
        v1 = abs(sv1[i])
        v2 = abs(sv2[i])
        if v1 + v2 < eps:
            continue
        if abs(v1 - v2) / (v1 + v2) > eps:
            print(f'absolute value error at {i}: expect {v1} have {v2}', file=sys.stderr)
            return False
        if v1 > maxv:
            maxv = v1
            maxi = i

    rel = sv1[maxi] / sv2[maxi]

    for i in range(2**n_qubits):
        v1 = sv1[i]
        v2 = sv2[i] * rel
        av = abs(v1)
        if av < eps:
            continue
        if abs(v1 - v2) / av > eps:
            print(f'value error at {i}: expect {v1} have {v2} (phase-corrected from {sv2[i]})', file=sys.stderr)
            print(f'phase determined by {maxi}: {sv1[maxi]} with secondary {sv2[maxi]} -> {sv2[maxi] * rel}', file=sys.stderr)
            return False

    print('ok', circuit.num_qubits, t2 / t1, file=sys.stderr)
    return True


def gen(n_qubits: int, n_gates: int) -> qiskit.QuantumCircuit:
    circuit = qiskit.QuantumCircuit(qiskit.QuantumRegister(n_qubits))

    def rand_qubits(k: int) -> list[qiskit.circuit.Qubit]:
        s: list[qiskit.circuit.Qubit] = []
        while len(s) < k:
            qi = random.choice(circuit.qubits)
            if qi in s:
                continue
            s.append(qi)
        return s

    for i in range(n_gates):
        r = random.randrange(100)
        if r < 20:
            gate = random.choice(SINGLE_QUBIT_GATES)
            circuit.append(gate, rand_qubits(1))
        elif r < 50:
            gate = random.choice(TWO_QUBIT_GATES)
            circuit.append(gate, rand_qubits(2))
        elif r < 99:
            gate = random.choice(THREE_QUBIT_GATES)
            circuit.append(gate, rand_qubits(3))
        else:
            c = qiskit.circuit.ClassicalRegister(1)
            circuit.add_register(c)
            circuit.append(qiskit.circuit.Measure(), rand_qubits(1), c)

    return circuit


def main() -> None:
    match sys.argv:
        case [arg0]:
            n = None
        case [arg0, val]:
            n = int(val)
        case _:
            raise RuntimeError(f'Usage: rand-circuit.py [N]')

    while True:
        try:
            n_qubits = random.choice([4, 5, 12, 16, 18])
            n_gates = 2**min(16, 28 - n_qubits)
            circuit = gen(n_qubits, n_gates)
            if not test(circuit):
                break
        except AssertionError:
            traceback.print_exc()
            break
        if n is None:
            continue
        n -= 1
        if n <= 0:
            return

    print('fail', file=sys.stderr)
    print(qiskit.qasm2.dumps(circuit))
    sys.exit(1)


main()
