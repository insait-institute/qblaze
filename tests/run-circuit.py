#!/usr/bin/python3
# SPDX-License-Identifier: Apache-2.0
import sys
import numpy

import qiskit
import qiskit.qasm2
import qiskit_aer

import qblaze.qiskit


def test(circuit: qiskit.QuantumCircuit) -> bool:
    n_qubits = len(circuit.qubits)
    circuit.save_statevector(pershot=True)

    sim1 = qiskit_aer.AerSimulator(method='statevector', precision='double', device='CPU', fusion_enable=False)
    sim2 = qblaze.qiskit.Backend(None)

    res1 = sim1.run(circuit, shots=1).result()

    if circuit.num_clbits > 0:
        [got] = res1.get_counts().keys()
        got = ''.join(got.split(' '))[::-1]
        got_clbits = {clbit: got[i] == '1' for i, clbit in enumerate(circuit.clbits)}
    else:
        got_clbits = {}

    res2 = sim2.run(circuit, shots=1, respect_barriers=True, force_clbits=got_clbits).result()

    [sv1] = res1.data()['statevector']
    [sv2] = res2.data()['statevector']
    sv1 = numpy.asarray(sv1)
    assert len(sv1) == 2**n_qubits
    assert len(sv2) == 2**n_qubits

    eps = 2**-24

    fail = False
    maxi = 0
    maxv = 0.0
    for i in range(2**n_qubits):
        v1 = abs(sv1[i])
        v2 = abs(sv2[i])
        if v1 + v2 < eps:
            continue
        if abs(v1 - v2) / (v1 + v2) > eps:
            print(f'absolute value error at {i}: expect {v1} have {v2}', file=sys.stderr)
            fail = True
            continue
        if v1 > maxv:
            maxv = v1
            maxi = i

    if fail:
        return False
    rel = sv1[maxi] / sv2[maxi]

    for i in range(2**n_qubits):
        v1 = sv1[i]
        v2 = sv2[i] * rel
        av = abs(v1)
        if av < eps:
            continue
        if abs(v1 - v2) / av > eps:
            print(f'value error at {i}: expect {v1} have {v2} (phase-corrected from {sv2[i]})', file=sys.stderr)
            fail = True
            continue

    if fail:
        print(f'phase determined by {maxi}: {sv1[maxi]} with secondary {sv2[maxi]} -> {sv2[maxi] * rel}', file=sys.stderr)
        return False

    return True


def main() -> None:
    [arg0, path] = sys.argv

    circuit = qiskit.qasm2.load(path, custom_instructions=(
        # TODO deal with `id` gates?
        qiskit.qasm2.CustomInstruction('sx', 0, 1, qiskit.circuit.library.SXGate, builtin=True),
        qiskit.qasm2.CustomInstruction('sxdg', 0, 1, qiskit.circuit.library.SXdgGate, builtin=True),
        qiskit.qasm2.CustomInstruction('p', 1, 1, qiskit.circuit.library.PhaseGate, builtin=True),
        qiskit.qasm2.CustomInstruction('u', 3, 1, qiskit.circuit.library.UGate, builtin=True),
        qiskit.qasm2.CustomInstruction('cp', 1, 2, qiskit.circuit.library.CPhaseGate, builtin=True),
        qiskit.qasm2.CustomInstruction('ccz', 0, 3, qiskit.circuit.library.CCZGate, builtin=True),
        qiskit.qasm2.CustomInstruction('rxx', 1, 2, qiskit.circuit.library.RXXGate, builtin=True),
        qiskit.qasm2.CustomInstruction('rzz', 1, 2, qiskit.circuit.library.RZZGate, builtin=True),
        qiskit.qasm2.CustomInstruction('swap', 0, 2, qiskit.circuit.library.SwapGate, builtin=True),
        qiskit.qasm2.CustomInstruction('cswap', 0, 3, qiskit.circuit.library.CSwapGate, builtin=True),
        qiskit.qasm2.CustomInstruction('cry', 1, 2, qiskit.circuit.library.CRYGate, builtin=True),
    ))

    num_clbits = 0
    for inst in circuit:
        if not isinstance(inst.operation, qiskit.circuit.Reset):
            continue
        num_clbits += len(inst.qubits)
    if num_clbits:
        mreg = qiskit.circuit.ClassicalRegister(num_clbits, name='mock_reset')
        circuit2 = qiskit.QuantumCircuit(*circuit.qregs, *circuit.cregs, mreg)
        circuit2.global_phase = circuit.global_phase
        num_clbits = 0
        for inst in circuit:
            if isinstance(inst.operation, qiskit.circuit.Reset):
                n2 = num_clbits + len(inst.qubits)
                clbits = mreg[num_clbits : n2]
                circuit2.append(qiskit.circuit.Measure(), inst.qubits, clbits)
                for (q, c) in zip(inst.qubits, clbits, strict=True):
                    circuit2.append(qiskit.circuit.library.XGate().c_if(c, True), [q], [])
                num_clbits = n2
            else:
                circuit2.append(inst.operation, inst.qubits, inst.clbits)
        assert num_clbits == len(mreg)

        circuit = circuit2

    if not test(circuit):
        print('FAIL')
        sys.exit(1)


main()
