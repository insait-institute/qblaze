#define _GNU_SOURCE
#define _USE_MATH_DEFINES
#include <qblaze.h>
#include <assert.h>
#include <math.h>
#include <stdio.h>
#include <unistd.h>

int main() {
	uint64_t random64[2];
	int r = getentropy(random64, sizeof(random64));
	if (r < 0) return 1;

	QBlazeSimulator *sim = qblaze_new(NULL);
	if (!sim) return 1;

	// create superposition: |00⟩ ↦ (|00⟩+|10⟩)/√2
	r = qblaze_apply_u3(sim, 0, M_PI_2, 0, M_PI); // H 0
	if (r < 0) goto fail;

	// entangle the two qubits: (|00⟩+|11⟩)/√2
	r = qblaze_apply_mcx(sim, (struct QBlazeControl[]){{0, true}}, 1, 1); // CX 0, 1
	if (r < 0) goto fail;

	// measure qubit 0
	r = qblaze_measure(sim, 0, random64[0], NULL, NULL);
	if (r < 0) goto fail;
	bool c0 = r;
	printf("Measured qubit 0 as %d\n", r);

	// measure qubit 1
	r = qblaze_measure(sim, 1, random64[1], NULL, NULL);
	if (r < 0) goto fail;
	bool c1 = r;
	printf("Measured qubit 1 as %d\n", r);

	assert(c0 == c1); // measurement outcomes are perfectly correlated

	qblaze_del(sim);
	return 0;

fail:
	fprintf(stderr, "Error %d\n", r);
	qblaze_del(sim);
	return 1;
}
