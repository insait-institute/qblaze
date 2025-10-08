import qblaze

sim = qblaze.Simulator()

sim.h(0)     # create superposition: |00⟩ ↦ (|00⟩+|10⟩)/√2
sim.cx(0, 1) # entangle the two qubits: (|00⟩+|11⟩)/√2

sim.dump()   # show state vector on stderr

# measure both qubits:
c0 = sim.measure(0)
c1 = sim.measure(1)

assert c0 == c1 # measurement outcomes are perfectly correlated

# reset qubits (just for demonstration):
if c0:
    sim.x(0)
if c1:
    sim.x(1)
