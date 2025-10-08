Q&A
###

Why attempt to simulate quantum algorithms on a classical computer?
===================================================================

There are multiple reasons why developers of quantum algorithms may choose to use a classical simulator:

 * Classical hardware is much more **abundant**, so it can be used to **debug** quantum algorithms **cheaply** on **small enough instances**.
 * Classical simulators allow testing in a **tightly controlled environment** that is not subject to hard-to-model sources of noise.
 * Intermediate states are **inspectable** (in contrast to quantum states on a quantum computer), which is helpful for **debugging** and **teaching**.
 * We can **mitigate the probabilistic nature** of quantum computing, by running a simulation with a **fixed random seed** or with **fixed measurement outcomes**, for reproducible runs.

Fast and scalable simulators such as qblaze increase the size of the smallest problem instances on which quantum algorithms are amenable to classical simulation.


What makes qblaze so fast and scalable?
=======================================

qblaze uses a representation based on basis vector/amplitude pairs, sorted by the basis vector bit pattern. This representation is operated on by algorithms that exhibit high spacial locality to get the most out of CPU caches. Furthermore, the algorithms are easily parallelizable without most of the bottlenecks suffered by approaches based on hash tables.


For which workloads is qblaze particularly helpful?
===================================================

As qblaze exploits sparsity of the state vector in the computational basis, it is particularly fast when the state vector is frequently at least somewhat sparse during the execution of the program. This is often the case for quantum algorithms that use ancilla qubits, or quantum algorithms that exploit quantum parallelism. State vector invariants that can be expressed as classical predicates over computational basis vectors also cause sparsity (for example, in modular arithmetic). Also, measurements in the computational basis with multiple likely possible outcomes will cause the state vector to collapse to one that is sparse in the computational basis. Furthermore, qblaze is particularly fast at evaluating large batches of phase/permutation gates (e.g. Z, CZ, X, CX, SWAP, ...).


What other approaches to simulation are there?
==============================================

If your state vector is dense (almost) all of the time, you may instead want to use a dense simulator or one based on tensor networks/matrix-product states. If your circuit consists of mostly Clifford gates, approaches based on stabilizer simulation may be right for you. If your state vector exhibits significant additional structure beyond sparsity which is easily represented in a binary decision diagram, BDD-based simulators may be helpful.


Do you support GPU acceleration or distributed setups?
======================================================

Not yet. For now, qblaze is CPU-only, on a single node.


How many qubits can be simulated?
=================================

There is a fixed limit of 1920 qubits by default, as this allows for a particularly fast implementation where a basis vector fits in CPU registers.
If you need more qubits, you can adjust the constant :code:`Qubit::MAX` in :file:`src/qubit.rs` and recompile, though performance may suffer.
