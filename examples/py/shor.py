import math
import random
import typing

import qblaze


def run_circuit(mod: int, a: int) -> tuple[int, int]:
    sim = qblaze.Simulator()
    assert 1 < a < mod - 1

    n_bits = mod.bit_length()
    m_bits = 2 * n_bits + 1

    val = list(range(n_bits))
    ctl_reg = n_bits
    addctl_reg = n_bits + 1
    mod_add_anc = n_bits + 2
    int_add_anc = n_bits + 3
    mod_add_arg = list(range(n_bits + 4, 2*n_bits + 4))
    int_geq_anc = mod_add_arg
    mod_mul_anc = list(range(2*n_bits + 4, 3*n_bits + 4))

    def int_xor(q: list[int], a: int, ctl: int) -> None:
        for i, qb in enumerate(q):
            if not (a & (1 << i)):
                continue
            sim.cx(ctl, qb)

    def int_add(q: list[int], a: list[int], carry: int | None = None) -> None:
        n = len(q)
        assert len(a) == n

        if n == 0:
            return

        if n == 1:
            if carry is not None:
                sim.ccx(a[0], q[0], carry)
            sim.cx(a[0], q[0])
            return

        if n == 2 and carry is None:
            sim.cx(a[1], q[1])
            sim.ccx(a[0], q[0], q[1])
            sim.cx(a[0], q[0])
            return

        anc = int_add_anc

        if n == 2:
            assert carry is not None
            sim.ccx(q[0], a[0], anc)
            sim.cx(a[1], anc)
            sim.cx(a[1], q[1])
            sim.cx(a[1], carry)
            sim.ccx(anc, q[1], carry)
            sim.cx(a[1], anc)
            sim.cx(anc, q[1])
            sim.ccx(q[0], a[0], anc)
            sim.cx(a[0], q[0])
            return

        sim.ccx(q[0], a[0], anc)
        sim.cx(a[1], anc)
        sim.cx(a[1], q[1])
        sim.ccx(anc, q[1], a[1])
        for i in range(2, n-1):
            sim.cx(a[i], a[i-1])
            sim.cx(a[i], q[i])
            sim.ccx(a[i-1], q[i], a[i])

        sim.cx(a[n-1], q[n-1])
        if carry is not None:
            sim.cx(a[n-1], carry)
            sim.cx(a[n-1], a[n-2])
            sim.ccx(a[n-2], q[n-1], carry)
            sim.cx(a[n-1], a[n-2])
        sim.cx(a[n-2], q[n-1])

        for i in range(n-2, 1, -1):
            sim.ccx(a[i-1], q[i], a[i])
            sim.cx(a[i], a[i-1])
            sim.cx(a[i-1], q[i])
        sim.ccx(anc, q[1], a[1])
        sim.cx(a[1], anc)
        sim.cx(anc, q[1])
        sim.ccx(q[0], a[0], anc)
        sim.cx(a[0], q[0])

    def int_geq(q: list[int], a: int, out: int, ctl: int) -> None:
        n = len(q)
        assert n == n_bits

        if a >= 2**n:
            return
        if a == 0:
            sim.cx(ctl, out)
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
            sim.ccx(ctl, q[0], out)
            return

        assert n >= 2
        assert a < 2**n-1
        assert not (a & 1)

        anc = [q[0], *int_geq_anc[:n-1]]

        for i in range(1, n):
            if not (a & (1 << i)):
                sim.x(q[i])
                sim.x(anc[i-1])
                sim.x(anc[i])
            sim.ccx(q[i], anc[i-1], anc[i])

        sim.ccx(ctl, anc[n-1], out)

        for i in range(1, n)[::-1]:
            sim.ccx(q[i], anc[i-1], anc[i])
            if not (a & (1 << i)):
                sim.x(anc[i])
                sim.x(anc[i-1])
                sim.x(q[i])

    def mod_add(q: list[int], a: int, ctl: int) -> None:
        if a == 0:
            return

        int_geq(q, mod - a, mod_add_anc, ctl=ctl)

        b = (2**n_bits - mod + a) ^ a
        int_xor(mod_add_arg, a, ctl)
        int_xor(mod_add_arg, b, mod_add_anc)

        top_bit = 1 << (n_bits - 1)
        if a < top_bit and b < top_bit:
            int_add(q[:-1], mod_add_arg[:-1], carry=q[-1])
        else:
            int_add(q, mod_add_arg)

        int_xor(mod_add_arg, b, mod_add_anc)
        int_xor(mod_add_arg, a, ctl)

        sim.cx(ctl, mod_add_anc)
        int_geq(q, a, mod_add_anc, ctl=ctl)


    def mod_sub(q: list[int], a: int, ctl: int) -> None:
        if a == 0:
            return
        mod_add(q, mod - a, ctl)

    def mod_mul(q: list[int], a: int, ctl: int) -> None:
        for i in range(n_bits):
            sim.ccx(ctl, q[i], addctl_reg)
            mod_add(mod_mul_anc, (a << i) % mod, ctl=addctl_reg)
            sim.ccx(ctl, q[i], addctl_reg)

        for i in range(n_bits):
            sim.cswap(ctl, q[i], mod_mul_anc[i])

        a_inv = pow(-a, -1, mod)
        for i in range(n_bits):
            sim.ccx(ctl, q[i], addctl_reg)
            mod_add(mod_mul_anc, (a_inv << i) % mod, ctl=addctl_reg)
            sim.ccx(ctl, q[i], addctl_reg)

    sim.x(val[0])
    period = 0
    for i in range(m_bits):
        sim.h(ctl_reg)
        mod_mul(val, pow(a, 2**(m_bits - 1 - i), mod), ctl=ctl_reg)
        for j in range(i):
            if period & (1 << j):
                sim.rz(ctl_reg, math.pi / 2**(i - j))
        sim.h(ctl_reg)
        if sim.measure(ctl_reg):
            period |= 1 << i
            sim.x(ctl_reg)

    return (period, m_bits)


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
        (m, prec) = run_circuit(mod, a)
        for (p, q) in continued_fraction_approx(m, 2**prec):
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
