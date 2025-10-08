import random
import typing
from qiskit.circuit import QuantumCircuit, QuantumRegister, Qubit, ClassicalRegister

import qblaze.qiskit


def deutsch_jozsa(
    n: int,
    oracle: typing.Callable[[QuantumCircuit, QuantumRegister, Qubit], None],
) -> None:
    x = QuantumRegister(n)
    y = QuantumRegister(1)
    r = ClassicalRegister(n)
    circ = QuantumCircuit(x, y, r)

    circ.x(y)
    circ.h(x)
    circ.h(y)
    oracle(circ, x, y[0])
    circ.h(x)
    circ.measure(x, r)

    backend = qblaze.qiskit.Backend()

    shots = 128
    result = backend.run(circ, shots=shots).result()

    zero_prob = result.data()['counts'].get('0x0', 0) / shots
    print(f'Measured probability for constant: {zero_prob}')


def constant_oracle(circ: QuantumCircuit, x: QuantumRegister, y: Qubit) -> None:
    circ.x(y)


def balanced_oracle(circ: QuantumCircuit, x: QuantumRegister, y: Qubit) -> None:
    circ.ccx(x[0], x[1], x[2])
    circ.cx(x[2], y)
    circ.ccx(x[0], x[1], x[2])
    circ.cx(x[3], y)


print('Constant oracle')
deutsch_jozsa(4, constant_oracle)
print('Balanced oracle')
deutsch_jozsa(4, balanced_oracle)
