from __future__ import annotations

import math
import random
import typing

import qiskit
import qblaze.qiskit


type Qubit = qiskit.circuit.Qubit
type QubitList = qiskit.QuantumRegister | list[Qubit]


def build_circuit(mod: int, a: int) -> qiskit.QuantumCircuit:
    assert 1 < a < mod - 1

    n_bits = mod.bit_length()
    m_bits = 2 * n_bits + 1

    val = qiskit.QuantumRegister(n_bits)
    period_reg = [qiskit.ClassicalRegister(1) for i in range(m_bits)]

    ctl_reg = qiskit.AncillaRegister(1)
    addctl_reg = qiskit.AncillaRegister(1)
    mod_add_aux = qiskit.AncillaRegister(1)
    int_add_anc = qiskit.AncillaRegister(1)
    mod_add_arg = qiskit.AncillaRegister(n_bits)
    int_geq_anc = mod_add_arg

    mod_mul_anc = qiskit.AncillaRegister(n_bits)
    circuit = qiskit.QuantumCircuit(val, ctl_reg, addctl_reg, mod_mul_anc, int_add_anc, mod_add_arg, mod_add_aux, *period_reg)

    def int_xor(q: QubitList, a: int, ctl: Qubit) -> None:
        for i, qb in enumerate(q):
            if not (a & (1 << i)):
                continue
            circuit.cx(ctl, qb)

    def int_add(q: QubitList, a: QubitList, carry: Qubit | None = None) -> None:
        assert carry is None or isinstance(carry, qiskit.circuit.quantumregister.Qubit)
        n = len(q)
        assert len(a) == n

        if n == 0:
            return

        if n == 1:
            if carry is not None:
                circuit.ccx(a, q, carry)
            circuit.cx(a, q)
            return

        if n == 2 and carry is None:
            circuit.cx(a[1], q[1])
            circuit.ccx(a[0], q[0], q[1])
            circuit.cx(a[0], q[0])
            return

        [anc] = int_add_anc

        if n == 2:
            assert carry is not None
            circuit.ccx(q[0], a[0], anc)
            circuit.cx(a[1], anc)
            circuit.cx(a[1], q[1])
            circuit.cx(a[1], carry)
            circuit.ccx(anc, q[1], carry)
            circuit.cx(a[1], anc)
            circuit.cx(anc, q[1])
            circuit.ccx(q[0], a[0], anc)
            circuit.cx(a[0], q[0])
            return

        circuit.ccx(q[0], a[0], anc)
        circuit.cx(a[1], anc)
        circuit.cx(a[1], q[1])
        circuit.ccx(anc, q[1], a[1])
        for i in range(2, n-1):
            circuit.cx(a[i], a[i-1])
            circuit.cx(a[i], q[i])
            circuit.ccx(a[i-1], q[i], a[i])

        circuit.cx(a[n-1], q[n-1])
        if carry is not None:
            circuit.cx(a[n-1], carry)
            circuit.cx(a[n-1], a[n-2])
            circuit.ccx(a[n-2], q[n-1], carry)
            circuit.cx(a[n-1], a[n-2])
        circuit.cx(a[n-2], q[n-1])

        for i in range(n-2, 1, -1):
            circuit.ccx(a[i-1], q[i], a[i])
            circuit.cx(a[i], a[i-1])
            circuit.cx(a[i-1], q[i])
        circuit.ccx(anc, q[1], a[1])
        circuit.cx(a[1], anc)
        circuit.cx(anc, q[1])
        circuit.ccx(q[0], a[0], anc)
        circuit.cx(a[0], q[0])

    def int_geq(q: qiskit.QuantumRegister, a: int, out: Qubit, ctl: Qubit) -> None:
        n = len(q)
        assert n == n_bits

        if a >= 2**n:
            return
        if a == 0:
            circuit.cx(ctl, out)
            return
        a -= 1

        i0 = 0
        while (a & (1 << i0)):
            i0 += 1
        q = q[i0:]
        n -= i0
        a >>= i0

        if n == 1:
            assert a == 0
            circuit.ccx(ctl, q[0], out)
            return

        assert n >= 2
        assert a < 2**n-1
        assert not (a & 1)

        anc = [q[0], *int_geq_anc[:n-1]]

        for i in range(1, n):
            if not (a & (1 << i)):
                circuit.x(q[i])
                circuit.x(anc[i-1])
                circuit.x(anc[i])
            circuit.ccx(q[i], anc[i-1], anc[i])

        circuit.ccx(ctl, anc[n-1], out)

        for i in range(1, n)[::-1]:
            circuit.ccx(q[i], anc[i-1], anc[i])
            if not (a & (1 << i)):
                circuit.x(anc[i])
                circuit.x(anc[i-1])
                circuit.x(q[i])

    def mod_add(q: qiskit.QuantumRegister, a: int, ctl: Qubit) -> None:
        if a == 0:
            return

        [overflow] = mod_add_aux
        int_geq(q, mod - a, overflow, ctl=ctl)

        b = (2**n_bits - mod + a) ^ a
        int_xor(mod_add_arg, a, ctl)
        int_xor(mod_add_arg, b, overflow)

        top_bit = 1 << (n_bits - 1)
        if a < top_bit and b < top_bit:
            int_add(q[:-1], mod_add_arg[:-1], carry=q[-1])
        else:
            int_add(q, mod_add_arg)

        int_xor(mod_add_arg, b, overflow)
        int_xor(mod_add_arg, a, ctl)

        circuit.cx(ctl, overflow)
        int_geq(q, a, overflow, ctl=ctl)


    def mod_sub(q: qiskit.QuantumRegister, a: int, ctl: Qubit) -> None:
        if a == 0:
            return
        mod_add(q, mod - a, ctl)

    def mod_mul(q: qiskit.QuantumRegister, a: int, ctl: Qubit) -> None:
        [flag] = addctl_reg
        for i in range(n_bits):
            circuit.ccx(ctl, q[i], flag)
            mod_add(mod_mul_anc, (a << i) % mod, ctl=flag)
            circuit.ccx(ctl, q[i], flag)

        for i in range(n_bits):
            circuit.cswap(ctl, q[i], mod_mul_anc[i])

        a_inv = pow(-a, -1, mod)
        for i in range(n_bits):
            circuit.ccx(ctl, q[i], flag)
            mod_add(mod_mul_anc, (a_inv << i) % mod, ctl=flag)
            circuit.ccx(ctl, q[i], flag)

    circuit.x(val[0])

    for i in range(m_bits):
        [ctl] = ctl_reg
        circuit.reset(ctl)
        circuit.h(ctl)
        mod_mul(val, pow(a, 2**(m_bits - 1 - i), mod), ctl=ctl)
        for j in range(i):
            phase_circ = qiskit.QuantumCircuit(ctl_reg)
            phase_circ.p(math.pi / 2**(i - j), ctl)
            circuit.append(qiskit.circuit.IfElseOp((period_reg[j], True), phase_circ), [ctl])
        circuit.h(ctl)
        circuit.measure(ctl, period_reg[i])

    return circuit


def run_circuit(circuit: qiskit.QuantumCircuit) -> int:
    backend = qblaze.qiskit.Backend()
    res = backend.run(circuit, shots=1).result()
    [(clbits, count)] = res.data()['counts'].items()
    assert count == 1
    assert clbits.startswith('0x')
    return int(clbits, 0)


def continued_fraction_approx(p: int, q: int) -> typing.Iterator[tuple[int, int]]:
    p0 = 0
    q0 = 1
    p1 = 1
    q1 = 0
    while q:
        d, r = divmod(p, q)
        p2 = p1 * d + p0
        q2 = q1 * d + q0
        yield (p2, q2)
        p0 = p1
        p1 = p2
        q0 = q1
        q1 = q2
        p = q
        q = r


def find_factor(mod: int) -> int:
    while True:
        a = random.randrange(2, mod - 1)
        g = math.gcd(a, mod)
        if g != 1:
            print(f'found {mod} = {g} * {mod // g} by random guessing')
            return g
        circuit = build_circuit(mod, a)
        m = run_circuit(circuit)
        for (p, q) in continued_fraction_approx(m, 2**circuit.num_clbits):
            if q >= mod:
                break
            v = pow(a, q, mod) - 1
            if v == 0:
                continue
            g = math.gcd(mod, v)
            if g != 1 and g != mod:
                print(f'found {mod} = {g} * {mod // g} using Shor\'s algorithm')
                return g
        print(f'failed a={a}; retrying')


find_factor(247)
find_factor(589)
find_factor(3599)
find_factor(14351)
find_factor(36089)
find_factor(216067)
find_factor(961307)
