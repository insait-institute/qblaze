import typing

import qblaze


ORACLE_N = 4


def deutsch_jozsa(oracle: typing.Callable[[qblaze.Simulator], None]) -> None:
    sim = qblaze.Simulator()

    sim.x(ORACLE_N)

    for i in range(ORACLE_N + 1):
        sim.h(i)

    oracle(sim)

    for i in range(ORACLE_N):
        sim.h(i)

    prob = 1.0
    num_ones = 0
    for i in range(ORACLE_N):
        (r, p0, p1) = sim.measure_ext(i)
        if r:
            prob *= p1
            num_ones += 1
        else:
            prob *= p0

    meaning = f'balanced' if num_ones else 'constant'
    print(f'Measured {num_ones} ones ({meaning}), likelihood {prob:f}')


def constant_oracle(sim: qblaze.Simulator) -> None:
    sim.x(ORACLE_N)


def balanced_oracle(sim: qblaze.Simulator) -> None:
    sim.ccx(0, 1, 2)
    sim.cx(2, ORACLE_N)
    sim.ccx(0, 1, 2)
    sim.cx(3, ORACLE_N)


print('Constant oracle')
deutsch_jozsa(constant_oracle)
print('Balanced oracle')
deutsch_jozsa(balanced_oracle)
