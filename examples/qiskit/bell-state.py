import qiskit

# Create a 2-qubit quantum circuit
q = qiskit.QuantumRegister(2)
r = qiskit.ClassicalRegister(2)
circ = qiskit.QuantumCircuit(q, r)

circ.h(q[0])        # create superposition: |00⟩ ↦ (|00⟩+|10⟩)/√2
circ.cx(q[0], q[1]) # entangle the two qubits: (|00⟩+|11⟩)/√2
circ.measure(q, r)  # measure both qubits

import qblaze.qiskit

# run 128 random simulations
backend = qblaze.qiskit.Backend()
shots = 128
result = backend.run(circ, shots=shots).result()
counts = result.data()['counts']
assert sorted(counts.keys()) == ['0x0', '0x3']

print(counts)
