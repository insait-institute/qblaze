import os
import sys

from qiskit import qasm3
import qblaze.qiskit

import pytest


circ_path = os.path.dirname(__file__) + '/circuits'
qasm3_circuits = []

for name in os.listdir(circ_path):
    if name.startswith('.'):
        continue
    if name.endswith('.qasm3'):
        with open(f'{circ_path}/{name}', 'r') as f:
            qasm3_circuits.append(f.read())
        continue
    raise RuntimeError(f'Bad file {name!r}')


@pytest.mark.parametrize('circ', qasm3_circuits)
def test_qasm3(circ: str) -> None:
    circ = qasm3.loads(circ)
    backend = qblaze.qiskit.Backend()
    result = backend.run(circ, shots=128).result()
    result.get_counts()
