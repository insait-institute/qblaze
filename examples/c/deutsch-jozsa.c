#define _GNU_SOURCE
#define _USE_MATH_DEFINES
#include <qblaze.h>
#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>

#define ORACLE_N 4

static uint64_t rand64(void) {
	uint64_t val;
	int r = getentropy(&val, sizeof(val));
	if (r < 0) {
		perror("getentropy");
		abort();
	}
	return val;
}

int deutsch_jozsa(int (*oracle)(QBlazeSimulator*)) {
	QBlazeSimulator *sim = qblaze_new(NULL);
	if (!sim) return QBLAZE_ERR_MEMORY;

	int r = qblaze_apply_u3(sim, ORACLE_N, M_PI, M_PI, 0); // X
	if (r < 0) goto fail;

	for (size_t i = 0; i <= ORACLE_N; i++) {
		r = qblaze_apply_u3(sim, i, M_PI_2, 0, M_PI); // H
		if (r < 0) goto fail;
	}

	r = oracle(sim);
	if (r < 0) goto fail;

	for (size_t i = 0; i < ORACLE_N; i++) {
		r = qblaze_apply_u3(sim, i, M_PI_2, 0, M_PI); // H
		if (r < 0) goto fail;
	}

	double prob = 1.0;
	size_t num_ones = 0;

	for (size_t i = 0; i < ORACLE_N; i++) {
		double p0, p1;
		int r = qblaze_measure(sim, i, rand64(), &p0, &p1);
		if (r < 0) goto fail;
		if (r) {
			num_ones++;
			prob *= p1;
		} else {
			prob *= p0;
		}
	}

	qblaze_del(sim);

	const char *meaning = num_ones ? "balanced" : "constant";

	printf("Measured %zu ones (%s), likelihood %lf\n", num_ones, meaning, prob);
	return 0;

fail:
	qblaze_del(sim);
	return r;
}

static int constant_oracle(QBlazeSimulator *sim) {
	return qblaze_apply_u3(sim, ORACLE_N, M_PI, M_PI, 0); // X
}

static int balanced_oracle(QBlazeSimulator *sim) {
	int r;
	r = qblaze_apply_mcx(sim, (struct QBlazeControl[]){{0, true}, {1, false}}, 2, 2);
	if (r < 0) return r;
	r = qblaze_apply_mcx(sim, (struct QBlazeControl[]){{2, true}}, 1, ORACLE_N);
	if (r < 0) return r;
	r = qblaze_apply_mcx(sim, (struct QBlazeControl[]){{0, true}, {1, false}}, 2, 2);
	if (r < 0) return r;

	return qblaze_apply_mcx(sim, (struct QBlazeControl[]){{3, true}}, 1, ORACLE_N);
}

int main() {
	int r;

	printf("Constant oracle\n");
	r = deutsch_jozsa(constant_oracle);
	if (r < 0) goto fail;

	printf("Balanced oracle\n");
	r = deutsch_jozsa(balanced_oracle);
	if (r < 0) goto fail;

	return 0;
fail:
	fprintf(stderr, "Error %d\n", r);
	return -1;
}
