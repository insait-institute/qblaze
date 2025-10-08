#pragma once
#include <complex.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/**
 * Errors.
 *
 * Upon failure, most operations return one of the following error codes:
 */
enum QBlazeError {
	QBLAZE_ERR_MEMORY      = -1, /**< Not enough memory. */
	QBLAZE_ERR_DOMAIN      = -2, /**< Parameter outside domain. */
	QBLAZE_ERR_QUBIT_INDEX = -3, /**< Invalid qubit index. */
	QBLAZE_ERR_QUBIT_USAGE = -4, /**< Invalid qubit usage. */
};

/**
 * Options.
 */
enum QBlazeOpt {
	/**
	 * Special option used to terminate an array of options.
	 */
	QBLAZE_OPT_END = 0,

	/**
	 * Dumps the simulator configuration to stderr.
	 *
	 * Default: no.
	 */
	QBLAZE_OPT_DUMP_CONFIG = 1,

	/**
	 * Hint about how many qubits will be used.
	 *
	 * This option determines how many bits are used to represent qubit indices.
	 * The representation is changed automatically when a qubit with a higher index is used.
	 * The maximum supported qubit count is returned by by qblaze_max_qubit_count().
	 *
	 * Default: 1.
	 */
	QBLAZE_OPT_QUBIT_COUNT = 2,

	/**
	 * The number of threads in the thread pool.
	 *
	 * Default: nproc.
	 */
	QBLAZE_OPT_THREAD_COUNT = 3,

	/**
	 * The chunk size for processing superposition-free gates (in bytes).
	 *
	 * If set to SIZE_MAX, then a single chunk is used.
	 *
	 * Default: smallest L2 cache size.
	 */
	QBLAZE_OPT_CHUNK_SIZE = 4,

	/**
	 * The state vector size after which non-temporal stores will be used (in bytes).
	 *
	 * Default: 2 * total L3 cache size.
	 */
	QBLAZE_OPT_NONTEMPORAL_THRESHOLD = 5,

	/**
	 * The state vector size after which multiple threads will be used (in bytes).
	 *
	 * Default: 64 KiB.
	 */
	QBLAZE_OPT_MULTITHREADING_THRESHOLD = 6,

	/**
	 * The minimum size of a thread work item (in bytes).
	 *
	 * Use to ensure that the work per item is larger that the communication overhead.
	 *
	 * Default: 1 KiB.
	 */
	QBLAZE_OPT_WORK_ITEM_MIN_SIZE = 7,

	/**
	 * The maximum size of a thread work item (in bytes).
	 *
	 * Use to ensure that there are enough items for threads that finish early.
	 *
	 * Default: 16 MiB.
	 */
	QBLAZE_OPT_WORK_ITEM_MAX_SIZE = 8,

    /** */
    QBLAZE_OPT__PERF_ENABLED = 9,
};

/**
 * An option configuration.
 */
struct QBlazeConfig {
	enum QBlazeOpt option;
	union {
		size_t as_size_t;
		void *as_ptr;
	} value;
};

/**
 * A complex number.  ABI compatible with `double _Complex`.
 */
typedef struct { double real, imag; } QBlazeComplex;

/**
 * Specifies a control condition.
 *
 * The condition is satisfied when the specified qubit has the specified value.
 */
struct QBlazeControl {
	size_t qubit;
	bool value;
};

/**
 * Return the maxumum supported qubit count.
 */
size_t qblaze_max_qubit_count(void);

/**
 * A qblaze simulator instance.
 */
struct QBlazeSimulator; /* needed for sphinx-c-autodoc */
typedef struct QBlazeSimulator QBlazeSimulator;

/**
 * Create a new simulator instance.
 *
 * The option configurations must be terminated by an QBLAZE_OPT_END.
 *
 * On error, return NULL.
 */
QBlazeSimulator *qblaze_new(const struct QBlazeConfig *opts);

/**
 * Delete a simulator instance.
 */
void qblaze_del(QBlazeSimulator *sim);

/**
 * Clone a simulator instance.
 *
 * On error, return NULL, leaving the state of the original simulator is undefined.
 */
QBlazeSimulator *qblaze_clone(QBlazeSimulator *sim);

/**
 * Apply all enqueued gates.
 */
int qblaze_flush(QBlazeSimulator *sim);

/**
 * Dump the state vector to stderr.
 */
int qblaze_dump(QBlazeSimulator *sim);

/**
 * Apply a general single-qubit gate.
 *
 * The definition is equivalent up to global phase to the OpenQASM `u` gate.
 */
int qblaze_apply_u3(QBlazeSimulator *sim, size_t target, double theta, double phi, double lambda);

/**
 * Apply a multiply controlled X gate.
 */
int qblaze_apply_mcx(QBlazeSimulator *sim, const struct QBlazeControl *controls, size_t count, size_t target);

/**
 * Apply a multiply controlled phase gate.
 */
int qblaze_apply_mcphase(QBlazeSimulator *sim, const struct QBlazeControl *controls, size_t count, double lambda);

/**
 * Apply a multiply controlled swap gate.
 */
int qblaze_apply_mcswap(QBlazeSimulator *sim, const struct QBlazeControl *controls, size_t count, size_t target1, size_t target2);

/**
 * Measure the target qubit.
 *
 * The state vector is probabilistically collapsed based on a random 64-bit input.
 * The probability for 0 is stored in 'p0', and the probability for 1 is stored in 'p1'.
 *
 * Return the measured value if successful, and an error code otherwise.
 */
int qblaze_measure(QBlazeSimulator *sim, size_t target, uint64_t random, double *p0, double *p1);

/**
 * Compute the measurement probabilities for the target qubit without measuring it.
 *
 * The probability for 0 is stored in 'p0', and the probability for 1 is stored in 'p1'.
 */
int qblaze_qubit_probs(QBlazeSimulator *sim, size_t target, double *p0, double *p1);

/**
 * Copy the state vector amplitudes to 'buf' but no more than 'len' of them.
 *
 * The amplitude of the 'i'th state vector is stored at position 'i',
 * where the 'k'th bit of 'i' equals the basis value of the 'k'th qubit.
 */
int qblaze_copy_amplitudes(QBlazeSimulator *sim, QBlazeComplex *buffer, size_t length);

/** */
char *_qblaze_perf(QBlazeSimulator *sim);


/**
 * Basis vector iterator.
 */
struct QBlazeIterator {
    QBlazeComplex amplitude;
    size_t qubit_count;
    uint8_t bitmap[];
};

/**
 * Create a new iterator over the state vector.  The iterator is uninitialized, so
 * `qblaze_iter_next` must be called.
 *
 * On error, return NULL.
 */
struct QBlazeIterator *qblaze_iter_new(QBlazeSimulator *sim);

/**
 * Advance the iterator to the next element of the state vector.  The iterator must have been
 * obtained from `qblaze_iter_new()`.  No operations can be applied on the state vector during
 * iteration. The iteration order is undefined.
 *
 * Returns true on success or false if the end of the state vector has been reached.
 */
bool qblaze_iter_next(struct QBlazeIterator *iter);

/**
 * Free an iterator.
 */
void qblaze_iter_del(struct QBlazeIterator *iter);


#ifdef __cplusplus
}
#endif
