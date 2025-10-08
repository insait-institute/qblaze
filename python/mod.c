#define _USE_MATH_DEFINES
#define PY_SSIZE_T_CLEAN
#include <Python.h>
#include <qblaze.h>
#include <math.h>
#include <stdbool.h>
#include <stdlib.h>
#include <string.h>

// Windows doesn't have getentropy...
#ifdef _WIN32
#define HAS_GETENTROPY 0
#else
#define HAS_GETENTROPY 1
#include <unistd.h>
#include <sys/random.h>
#endif


#define MOD_IDENTS \
	X(dump_config) \
	X(qubit_count) \
	X(thread_count) \
	X(chunk_size) \
	X(nontemporal_threshold) \
	X(multithreading_threshold)	\
	X(work_item_min_size) \
	X(work_item_max_size) \
	X(_perf_enabled) \
	X(__version__)

#define SIMULATOR_OPTS \
	X(dump_config, QBLAZE_OPT_DUMP_CONFIG, opt_bool) \
	X(qubit_count, QBLAZE_OPT_QUBIT_COUNT, opt_uint) \
	X(thread_count, QBLAZE_OPT_THREAD_COUNT, opt_uint) \
	X(chunk_size, QBLAZE_OPT_CHUNK_SIZE, opt_size) \
	X(multithreading_threshold, QBLAZE_OPT_MULTITHREADING_THRESHOLD, opt_size) \
	X(work_item_min_size, QBLAZE_OPT_WORK_ITEM_MIN_SIZE, opt_size) \
	X(work_item_max_size, QBLAZE_OPT_WORK_ITEM_MAX_SIZE, opt_size) \
	X(_perf_enabled, QBLAZE_OPT__PERF_ENABLED, opt_bool)


typedef struct ModState {
#define X(name) PyObject *id_##name;
MOD_IDENTS
#undef X

	PyObject *s_version;
	PyTypeObject *ty_Simulator;
	PyTypeObject *ty_Iterator;
} ModState;

static void mod_free(void *stv);
static int mod_exec(PyObject *mod);

static PyModuleDef mod_def = {
	PyModuleDef_HEAD_INIT,
	.m_name = "qblaze",
	.m_size = sizeof(ModState),
	.m_free = &mod_free,
	.m_slots = (PyModuleDef_Slot[]) {
		{Py_mod_exec, &mod_exec},
#if (defined(Py_LIMITED_API) ? Py_LIMITED_API : PY_VERSION_HEX) >= 0x030c0000
		{Py_mod_multiple_interpreters, Py_MOD_PER_INTERPRETER_GIL_SUPPORTED},
#endif
		{0},
	},
};


typedef struct SimulatorObject {
	PyObject_HEAD
	QBlazeSimulator *sim;
	struct IteratorObject *iter_obj;
} SimulatorObject;

typedef struct IteratorObject {
	PyObject_HEAD
	struct QBlazeIterator *iter;
	struct SimulatorObject *sim_obj;
} IteratorObject;


static size_t decode_size_t(PyObject *v, const char *name) {
	if (!PyLong_CheckExact(v)) {
		PyErr_Format(PyExc_TypeError, "%s: Expected int", name);
		return (size_t)-1;
	}
	size_t size = PyLong_AsSize_t(v);
	if (size == (size_t)-1 && PyErr_Occurred()) {
		if (PyErr_ExceptionMatches(PyExc_OverflowError)) {
			PyErr_Clear();
			PyErr_Format(PyExc_OverflowError, "%s: Out of range", name);
		}
		return (size_t)-1;
	}
	return size;
}

static double decode_double(PyObject *v, const char *name) {
	if (PyFloat_CheckExact(v)) {
		double r = PyFloat_AsDouble(v);
		if (!isfinite(r)) {
			PyErr_Format(PyExc_ValueError, "%s: Expected finite number", name);
			return NAN;
		}
		return r;
	}
	// Allow `0`, but not `False`.
	// FIXME: Maybe only allow the `int` 0, and enforce the use of `float` for other values?
	if (PyLong_CheckExact(v)) {
		double r = PyLong_AsDouble(v);
		if (r == -1.0 && PyErr_Occurred()) {
			PyErr_Clear();
			PyErr_Format(PyExc_OverflowError, "%s: Out of range", name);
			return NAN;
		}
		return r;
	}
	PyErr_Format(PyExc_TypeError, "%s: Expected float", name);
	return NAN;
}

static int decode_control_item(PyObject *ctl, struct QBlazeControl *out) {
	if (!PyTuple_CheckExact(ctl) || PyTuple_Size(ctl) != 2) {
		PyErr_SetString(PyExc_TypeError, "controls[]: Expected tuple of size 2");
		return -1;
	}
	size_t q = decode_size_t(PyTuple_GetItem(ctl, 0), "controls[][0]");
	if (q == (size_t)-1 && PyErr_Occurred()) return -1;
	PyObject *vp = PyTuple_GetItem(ctl, 1);
	bool v;
	if (Py_IsFalse(vp)) {
		v = false;
	} else if (Py_IsTrue(vp)) {
		v = true;
	} else {
		PyErr_SetString(PyExc_TypeError, "controls[][1]: Expected bool");
		return -1;
	}
	*out = (struct QBlazeControl) { q, v };
	return 0;
}

static Py_ssize_t decode_control_list(PyObject *seq, struct QBlazeControl **out) {
	if (Py_IsNone(seq)) {
		*out = NULL;
		return 0;
	}
	if (!PyList_CheckExact(seq)) {
		PyErr_SetString(PyExc_TypeError, "controls: Expected list");
		return -1;
	}
	size_t n = PyList_Size(seq);
	if (n > 4096) {
		PyErr_SetString(PyExc_ValueError, "Too many controls");
		return -1;
	}
	if (n == 0) {
		*out = NULL;
		return 0;
	}
	struct QBlazeControl *ctls = malloc(n * sizeof(struct QBlazeControl));
	for(size_t i = 0; i < n; i++) {
		int r = decode_control_item(PyList_GetItem(seq, i), ctls + i);
		if (r < 0) {
			free(ctls);
			return -1;
		}
	}
	*out = ctls;
	return (Py_ssize_t)n;
}

static ModState *mod_get(PyTypeObject *ty) {
	PyObject *mod = PyType_GetModule(ty);
	if (PyModule_GetDef(mod) != &mod_def) {
		Py_FatalError("Type not in qblaze?");
	}
	ModState *st = PyModule_GetState(mod);
	if (!st) {
		Py_FatalError("Module not initialized?");
	}
	return st;
}

static PyObject *Simulator_max_qubit_count(PyObject *ty, PyObject *arg) {
	(void)ty;
	(void)arg;
	return PyLong_FromUnsignedLong(qblaze_max_qubit_count());
}

static void Simulator_err_concurrent(void) {
	PyErr_SetString(PyExc_RuntimeError, "Concurrent use");
}

static void Simulator_err_code(int r) {
	switch(r) {
		case QBLAZE_ERR_MEMORY:
			PyErr_NoMemory();
			break;
		case QBLAZE_ERR_DOMAIN:
			PyErr_SetString(PyExc_ValueError, "Parameter outside domain");
			break;
		case QBLAZE_ERR_QUBIT_INDEX:
			PyErr_SetString(PyExc_ValueError, "Invalid qubit index");
			break;
		case QBLAZE_ERR_QUBIT_USAGE:
			PyErr_SetString(PyExc_ValueError, "Invalid qubit usage");
			break;
		default:
			PyErr_SetNone(PyExc_ValueError);
			break;
	}
}

static int opt_bool(PyObject *obj, const char *name, struct QBlazeConfig *out) {
	bool v;
	if (Py_IsFalse(obj)) {
		v = 0;
	} else if (Py_IsTrue(obj)) {
		v = 1;
	} else {
		PyErr_Format(PyExc_TypeError, "%s: Expected bool", name);
		return -1;
	}
	out->value.as_size_t = v;
	return 1;
}

static int opt_uint(PyObject *obj, const char *name, struct QBlazeConfig *out) {
	size_t v = decode_size_t(obj, name);
	if (v == (size_t)-1 && PyErr_Occurred()) return -1;
	out->value.as_size_t = v;
	return 1;
}

static int opt_size(PyObject *v, const char *name, struct QBlazeConfig *out) {
	if (!PyLong_CheckExact(v)) {
		PyErr_Format(PyExc_TypeError, "%s: Expected int", name);
		return -1;
	}
	size_t sz;
	long r = PyLong_AsSsize_t(v);
	if (r >= 0) {
		if (r == 0) return 0;
		sz = r;
	} else if (r != -1 || !PyErr_Occurred()) {
		sz = SIZE_MAX;
	} else {
		PyErr_Clear();
		PyErr_Format(PyExc_OverflowError, "%s: Out of range", name);
		return -1;
	}
	out->value.as_size_t = sz;
	return 1;
}

inline static void Simulator_init(SimulatorObject *self, QBlazeSimulator *sim) {
	self->sim = sim;
	self->iter_obj = NULL;
}

static void Simulator_invalidate_iterator(SimulatorObject *so, IteratorObject *io) {
	struct QBlazeIterator *iter = io->iter;
	assert(iter);
	io->iter = NULL;
	io->sim_obj = NULL;
	so->iter_obj = NULL;
	qblaze_iter_del(iter);
	Py_DECREF(so);
}

inline static QBlazeSimulator *Simulator_acquire(SimulatorObject *self) {
	QBlazeSimulator *sim = self->sim;
	if(!sim) {
		Simulator_err_concurrent();
		return NULL;
	}
	self->sim = NULL;
	IteratorObject *io = self->iter_obj;
	if(io) {
		assert(io->sim_obj == self);
		if (Py_REFCNT(&self->ob_base) <= 1) {
			Py_FatalError("Dropping last reference to simulator in acquire");
		}
		Simulator_invalidate_iterator(self, io);
	}
	return sim;
}

inline static void Simulator_release(SimulatorObject *self, QBlazeSimulator *sim) {
	if(!sim) abort();
	if(self->sim) abort();
	self->sim = sim;
}

static PyObject *Simulator_new(PyTypeObject *ty, PyObject *args, PyObject *kwargs) {
	ModState *st = mod_get(ty);

	if (PyTuple_Size(args) != 0) {
		PyErr_SetString(PyExc_TypeError, "This method takes no positional arguments");
		return NULL;
	}

	struct QBlazeConfig config[
#define X(name, opt, parse) 1+
SIMULATOR_OPTS
#undef X
	1];
	size_t n = 0;

	if (kwargs) {
		PyObject *val;
		Py_ssize_t n_dict = 0;

#define X(name, opt, parse) \
		if((val = PyDict_GetItem(kwargs, st->id_##name))) { \
			n_dict++; \
			if (!Py_IsNone(val)) { \
				struct QBlazeConfig *cfg = &config[n]; \
				cfg->option = opt; \
				int r = parse(val, #name, cfg); \
				if (r < 0) return NULL; \
				if (r != 0) n++; \
			} \
		}
SIMULATOR_OPTS
#undef X

		if (PyDict_Size(kwargs) != n_dict) {
			if (PyDict_Size(kwargs) < n_dict) {
				Py_FatalError("Got more options than dict size?");
			}
			Py_ssize_t pos = 0;
			PyObject *key;
			while (PyDict_Next(kwargs, &pos, &key, NULL)) {
#define X(name, opt, parse) if (PyUnicode_Compare(key, st->id_##name) == 0) continue;
SIMULATOR_OPTS
#undef X
				PyErr_Format(PyExc_TypeError, "Unexpected keyword argument %R", key);
				return NULL;
			}
			Py_FatalError("Got fewer options than dict size, but no unknown options?");
		}
	}

	config[n].option = QBLAZE_OPT_END;
	QBlazeSimulator *sim = qblaze_new(config);
	if (!sim) {
		return PyErr_NoMemory();
	}

	SimulatorObject *self = PyObject_NEW(SimulatorObject, ty);
	if (!self) {
		qblaze_del(sim);
		return PyErr_NoMemory();
	}
	Simulator_init(self, sim);
	return &self->ob_base;
}

static void Simulator_dealloc(PyObject *o) {
	SimulatorObject *self = (SimulatorObject*)o;
	QBlazeSimulator *sim = Simulator_acquire(self);
	if(!sim) {
		Py_FatalError("Simulator_dealloc while locked");
	}
	qblaze_del(sim);
	PyObject_Del(o);
}

static PyObject *Simulator_clone(PyObject *o, PyObject *arg) {
	SimulatorObject *self = (SimulatorObject*)o;
	(void)arg;
	// If iterators exist, the simulator is flushed.
	QBlazeSimulator *sim = Simulator_acquire(self);
	QBlazeSimulator *rsim = qblaze_clone(sim);
	Simulator_release(self, sim);
	if (!rsim) {
		return PyErr_NoMemory();
	}

	SimulatorObject *ro = PyObject_NEW(SimulatorObject, Py_TYPE(o));
	if(!ro) {
		qblaze_del(rsim);
		return PyErr_NoMemory();
	}
	Simulator_init(ro, rsim);
	return (PyObject*)ro;
}

static PyObject *Simulator_iter(PyObject *self_obj, PyObject *arg) {
	SimulatorObject *self = (SimulatorObject*)self_obj;
	(void)arg;
	ModState *st = mod_get(Py_TYPE(&self->ob_base));

	QBlazeSimulator *sim = Simulator_acquire(self);
	IteratorObject *io = NULL;

	int r;
	Py_BEGIN_ALLOW_THREADS
		r = qblaze_flush(sim);
	Py_END_ALLOW_THREADS

	if (r < 0) goto done;
	struct QBlazeIterator *iter = qblaze_iter_new(sim);
	if (!iter) goto done;
	io = PyObject_NEW(IteratorObject, st->ty_Iterator);
	if (!io) {
		qblaze_iter_del(iter);
		goto done;
	}
	Py_INCREF(&self->ob_base);
	io->iter = iter;
	self->iter_obj = io;
	io->sim_obj = self;

done:
	Simulator_release(self, sim);
	return &io->ob_base;
}


#define DO_U3(r, ...) do { \
	QBlazeSimulator *sim = Simulator_acquire(self); \
	if(!sim) { \
		r = -1; \
	} else { \
		r = qblaze_apply_u3(sim, __VA_ARGS__); \
		if(r < 0) Simulator_err_code(r); \
		Simulator_release(self, sim); \
	} \
} while(0)

#define DEF_SINGLE_QUBIT(name, theta, phi, lam) \
	static PyObject *Simulator_##name(PyObject *o, PyObject *arg) { \
		SimulatorObject *self = (SimulatorObject*)o; \
		size_t q = decode_size_t(arg, "target"); \
		if(PyErr_Occurred()) return NULL;	 \
		int r; \
		DO_U3(r, q, theta, phi, lam); \
		if(r < 0) return NULL; \
		Py_RETURN_NONE; \
	}

DEF_SINGLE_QUBIT(x, M_PI, M_PI, 0)
DEF_SINGLE_QUBIT(y, M_PI, 0, 0)
DEF_SINGLE_QUBIT(z, 0, M_PI, 0)
DEF_SINGLE_QUBIT(h, M_PI_2, 0, M_PI)
DEF_SINGLE_QUBIT(s, 0, M_PI_2, 0)
DEF_SINGLE_QUBIT(sdg, 0, -M_PI_2, 0)
DEF_SINGLE_QUBIT(t, 0, M_PI_4, 0)
DEF_SINGLE_QUBIT(tdg, 0, -M_PI_4, 0)

static PyObject *Simulator_u3(PyObject *o, PyObject *const *args, Py_ssize_t nargs) {
	SimulatorObject *self = (SimulatorObject*)o;
	if (nargs != 4) {
		PyErr_SetString(PyExc_TypeError, "Expected four arguments");
		return NULL;
	}
	size_t q = decode_size_t(args[0], "target");
	if (q == (size_t)-1 && PyErr_Occurred()) return NULL;
	double theta = decode_double(args[1], "theta");
	if (isnan(theta)) return NULL;
	double phi = decode_double(args[2], "phi");
	if (isnan(phi)) return NULL;
	double lam = decode_double(args[3], "lam");
	if (isnan(lam)) return NULL;
	int r;
	DO_U3(r, q, theta, phi, lam);
	if(r < 0) return NULL;
	Py_RETURN_NONE;
}

static PyObject *Simulator_rx(PyObject *o, PyObject *const *args, Py_ssize_t nargs) {
	SimulatorObject *self = (SimulatorObject*)o;
	if (nargs != 2) {
		PyErr_SetString(PyExc_TypeError, "Expected two arguments");
		return NULL;
	}
	size_t q = decode_size_t(args[0], "target");
	if (q == (size_t)-1 && PyErr_Occurred()) return NULL;
	double theta = decode_double(args[1], "theta");
	if (isnan(theta)) return NULL;
	int r;
	DO_U3(r, q, theta, -M_PI_2, M_PI_2);
	if(r < 0) return NULL;
	Py_RETURN_NONE;
}

static PyObject *Simulator_ry(PyObject *o, PyObject *const *args, Py_ssize_t nargs) {
	SimulatorObject *self = (SimulatorObject*)o;
	if (nargs != 2) {
		PyErr_SetString(PyExc_TypeError, "Expected two arguments");
		return NULL;
	}
	size_t q = decode_size_t(args[0], "target");
	if (q == (size_t)-1 && PyErr_Occurred()) return NULL;
	double theta = decode_double(args[1], "theta");
	if (isnan(theta)) return NULL;
	int r;
	DO_U3(r, q, theta, 0.0, 0.0);
	if(r < 0) return NULL;
	Py_RETURN_NONE;
}

static PyObject *Simulator_rz(PyObject *o, PyObject *const *args, Py_ssize_t nargs) {
	SimulatorObject *self = (SimulatorObject*)o;
	if (nargs != 2) {
		PyErr_SetString(PyExc_TypeError, "Expected two arguments");
		return NULL;
	}
	size_t q = decode_size_t(args[0], "target");
	if (q == (size_t)-1 && PyErr_Occurred()) return NULL;
	double theta = decode_double(args[1], "theta");
	if (isnan(theta)) return NULL;
	int r;
	DO_U3(r, q, 0.0, theta, 0.0);
	if(r < 0) return NULL;
	Py_RETURN_NONE;
}


#define DO_SLOW(r, f, ...) do { \
	QBlazeSimulator *sim = Simulator_acquire(self); \
	if(!sim) { \
		r = -1; \
	} else { \
		Py_BEGIN_ALLOW_THREADS \
			r = f(sim ,##__VA_ARGS__); \
		Py_END_ALLOW_THREADS \
		Simulator_release(self, sim); \
		if(r < 0) Simulator_err_code(r); \
	} \
} while(0)

static PyObject *Simulator_flush(PyObject *o, PyObject *arg) {
	SimulatorObject *self = (SimulatorObject*)o;
	(void)arg;
	int r;
	DO_SLOW(r, qblaze_flush);
	if(r < 0) return NULL; \
	Py_RETURN_NONE;
}

static PyObject *Simulator_dump(PyObject *o, PyObject *arg) {
	SimulatorObject *self = (SimulatorObject*)o;
	(void)arg;
	int r;
	DO_SLOW(r, qblaze_dump);
	if(r < 0) return NULL;
	Py_RETURN_NONE;
}

static PyObject *Simulator__perf(PyObject *o, PyObject *arg) {
	SimulatorObject *self = (SimulatorObject*)o;
	(void)arg;
	QBlazeSimulator *sim = Simulator_acquire(self);
	if(!sim) {
		return NULL;
	}
	char *data = _qblaze_perf(sim);
	Simulator_release(self, sim);
	if (!data) {
		return PyErr_NoMemory();
	}

	PyObject *ro = PyUnicode_FromString(data);
	free(data);
	return ro;
}

static PyObject *Simulator_cx(PyObject *o, PyObject *const *args, Py_ssize_t nargs) {
	SimulatorObject *self = (SimulatorObject*)o;
	if (nargs != 2) {
		PyErr_SetString(PyExc_TypeError, "Expected two arguments");
		return NULL;
	}
	size_t q1 = decode_size_t(args[0], "control");
	if (q1 == (size_t)-1 && PyErr_Occurred()) return NULL;
	size_t q2 = decode_size_t(args[1], "target");
	if (q2 == (size_t)-1 && PyErr_Occurred()) return NULL;
	struct QBlazeControl ctl = {q1, true};
	int r;
	DO_SLOW(r, qblaze_apply_mcx, &ctl, 1, q2);
	if(r < 0) return NULL;
	Py_RETURN_NONE;
}

static PyObject *Simulator_ccx(PyObject *o, PyObject *const *args, Py_ssize_t nargs) {
	SimulatorObject *self = (SimulatorObject*)o;
	if (nargs != 3) {
		PyErr_SetString(PyExc_TypeError, "Expected three arguments");
		return NULL;
	}
	size_t q1 = decode_size_t(args[0], "control1");
	if (q1 == (size_t)-1 && PyErr_Occurred()) return NULL;
	size_t q2 = decode_size_t(args[1], "control2");
	if (q2 == (size_t)-1 && PyErr_Occurred()) return NULL;
	size_t q3 = decode_size_t(args[2], "target");
	if (q3 == (size_t)-1 && PyErr_Occurred()) return NULL;
	struct QBlazeControl ctl[2] = {
		{q1, true},
		{q2, true},
	};
	int r;
	DO_SLOW(r, qblaze_apply_mcx, ctl, 2, q3);
	if(r < 0) return NULL;
	Py_RETURN_NONE;
}

static PyObject *Simulator_mcx(PyObject *o, PyObject *const *args, Py_ssize_t nargs) {
	SimulatorObject *self = (SimulatorObject*)o;
	if (nargs != 2) {
		PyErr_SetString(PyExc_TypeError, "Expected two arguments");
		return NULL;
	}
	size_t q = decode_size_t(args[1], "target");
	if (q == (size_t)-1 && PyErr_Occurred()) return NULL;
	struct QBlazeControl *ctl;
	Py_ssize_t ctln = decode_control_list(args[0], &ctl);
	if (ctln < 0) return NULL;
	int r;
	DO_SLOW(r, qblaze_apply_mcx, ctl, ctln, q);
	free(ctl);
	if(r < 0) return NULL;
	Py_RETURN_NONE;
}

static PyObject *Simulator_swap(PyObject *o, PyObject *const *args, Py_ssize_t nargs) {
	SimulatorObject *self = (SimulatorObject*)o;
	if (nargs != 2) {
		PyErr_SetString(PyExc_TypeError, "Expected two arguments");
		return NULL;
	}
	size_t q1 = decode_size_t(args[0], "target1");
	if (q1 == (size_t)-1 && PyErr_Occurred()) return NULL;
	size_t q2 = decode_size_t(args[1], "target2");
	if (q2 == (size_t)-1 && PyErr_Occurred()) return NULL;
	int r;
	DO_SLOW(r, qblaze_apply_mcswap, NULL, 0, q1, q2);
	if(r < 0) return NULL;
	Py_RETURN_NONE;
}

static PyObject *Simulator_cswap(PyObject *o, PyObject *const *args, Py_ssize_t nargs) {
	SimulatorObject *self = (SimulatorObject*)o;
	if (nargs != 3) {
		PyErr_SetString(PyExc_TypeError, "Expected three arguments");
		return NULL;
	}
	size_t qc = decode_size_t(args[0], "control");
	if (qc == (size_t)-1 && PyErr_Occurred()) return NULL;
	size_t q1 = decode_size_t(args[1], "target1");
	if (q1 == (size_t)-1 && PyErr_Occurred()) return NULL;
	size_t q2 = decode_size_t(args[2], "target2");
	if (q2 == (size_t)-1 && PyErr_Occurred()) return NULL;
	struct QBlazeControl ctl = { qc, true };
	int r;
	DO_SLOW(r, qblaze_apply_mcswap, &ctl, 1, q1, q2);
	if(r < 0) return NULL;
	Py_RETURN_NONE;
}

static PyObject *Simulator_mcswap(PyObject *o, PyObject *const *args, Py_ssize_t nargs) {
	SimulatorObject *self = (SimulatorObject*)o;
	if (nargs != 3) {
		PyErr_SetString(PyExc_TypeError, "Expected three arguments");
		return NULL;
	}
	size_t q1 = decode_size_t(args[1], "target1");
	if (q1 == (size_t)-1 && PyErr_Occurred()) return NULL;
	size_t q2 = decode_size_t(args[2], "target2");
	if (q2 == (size_t)-1 && PyErr_Occurred()) return NULL;
	struct QBlazeControl *ctl;
	Py_ssize_t ctln = decode_control_list(args[0], &ctl);
	if (ctln < 0) return NULL;
	int r;
	DO_SLOW(r, qblaze_apply_mcswap, ctl, ctln, q1, q2);
	free(ctl);
	if(r < 0) return NULL;
	Py_RETURN_NONE;
}

static PyObject *Simulator_cz(PyObject *o, PyObject *const *args, Py_ssize_t nargs) {
	SimulatorObject *self = (SimulatorObject*)o;
	if (nargs != 2) {
		PyErr_SetString(PyExc_TypeError, "Expected two arguments");
		return NULL;
	}
	size_t q1 = decode_size_t(args[0], "control");
	if (q1 == (size_t)-1 && PyErr_Occurred()) return NULL;
	size_t q2 = decode_size_t(args[1], "target");
	if (q2 == (size_t)-1 && PyErr_Occurred()) return NULL;
	struct QBlazeControl ctl[2] = {
		{q1, true},
		{q2, true},
	};
	int r;
	DO_SLOW(r, qblaze_apply_mcphase, ctl, 2, M_PI);
	if(r < 0) return NULL;
	Py_RETURN_NONE;
}

static PyObject *Simulator_mcphase(PyObject *o, PyObject *const *args, Py_ssize_t nargs) {
	SimulatorObject *self = (SimulatorObject*)o;
	if (nargs != 2) {
		PyErr_SetString(PyExc_TypeError, "Expected two arguments");
		return NULL;
	}
	double lam = decode_double(args[1], "theta");
	if (isnan(lam)) return NULL;
	struct QBlazeControl *ctl;
	Py_ssize_t ctln = decode_control_list(args[0], &ctl);
	if (ctln < 0) return NULL;
	int r;
	DO_SLOW(r, qblaze_apply_mcphase, ctl, ctln, lam);
	free(ctl);
	if(r < 0) return NULL;
	Py_RETURN_NONE;
}

static int Simulator__measure(PyObject *o, PyObject *const *args, Py_ssize_t nargs, double *p0, double *p1) {
	SimulatorObject *self = (SimulatorObject*)o;
	if (nargs < 1 || nargs > 2) {
		PyErr_SetString(PyExc_TypeError, "Expected one or two arguments");
		return -1;
	}
	size_t q = decode_size_t(args[0], "target");
	if (q == (size_t)-1 && PyErr_Occurred()) return -1;

	unsigned long long rnd;
	if (nargs < 2 || Py_IsNone(args[1])) {
#if HAS_GETENTROPY
		int r = getentropy(&rnd, sizeof(rnd));
		if (r < 0) {
			PyErr_SetFromErrno(PyExc_OSError);
			return -1;
		}
#else
		rnd = 0;
		for (size_t i = 0; i < 5; i++) {
			rnd <<= 13;
			rnd ^= rand();
		}
#endif
	} else {
		if (!PyLong_CheckExact(args[1])) {
			PyErr_SetString(PyExc_TypeError, "seed: Expected int");
			return -1;
		}
		rnd = PyLong_AsUnsignedLongLong(args[1]);
		if (rnd == (unsigned long long)-1 && PyErr_Occurred()) {
			return -1;
		}
	}

	int r;
	DO_SLOW(r, qblaze_measure, q, rnd, p0, p1);
	return r;
}

static PyObject *Simulator_measure(PyObject *o, PyObject *const *args, Py_ssize_t nargs) {
	double p0, p1;
	int r = Simulator__measure(o, args, nargs, &p0, &p1);
	if(r < 0) return NULL;
	return PyBool_FromLong(r);
}

static PyObject *Simulator_measure_ext(PyObject *o, PyObject *const *args, Py_ssize_t nargs) {
	double p0, p1;
	int r = Simulator__measure(o, args, nargs, &p0, &p1);
	if(r < 0) return NULL;

	PyObject *p0o = PyFloat_FromDouble(p0);
	if (!p0o) return NULL;
	PyObject *p1o = PyFloat_FromDouble(p1);
	if (!p1o) {
		Py_DECREF(p0o);
		return NULL;
	}

	PyObject *ret = PyTuple_New(3);
	if (!ret) {
		Py_DECREF(p0o);
		Py_DECREF(p1o);
		return NULL;
	}
	r = PyTuple_SetItem(ret, 0, PyBool_FromLong(r));
	if (r < 0) Py_FatalError("Index 0 out of bounds in a 3-element tuple?");
	r = PyTuple_SetItem(ret, 1, p0o);
	if (r < 0) Py_FatalError("Index 1 out of bounds in a 3-element tuple?");
	r = PyTuple_SetItem(ret, 2, p1o);
	if (r < 0) Py_FatalError("Index 2 out of bounds in a 3-element tuple?");
	return ret;
}

static PyObject *Simulator_qubit_probs(PyObject *o, PyObject *arg) {
	SimulatorObject *self = (SimulatorObject*)o;
	size_t q = decode_size_t(arg, "target");
	if (q == (size_t)-1 && PyErr_Occurred()) return NULL;

	double p0, p1;
	int r;
	DO_SLOW(r, qblaze_qubit_probs, q, &p0, &p1);
	if(r < 0) return NULL;

	PyObject *p0o = PyFloat_FromDouble(p0);
	if (!p0o) return NULL;
	PyObject *p1o = PyFloat_FromDouble(p1);
	if (!p1o) {
		Py_DECREF(p0o);
		return NULL;
	}

	PyObject *ret = PyTuple_New(2);
	if (!ret) {
		Py_DECREF(p0o);
		Py_DECREF(p1o);
		return NULL;
	}
	r = PyTuple_SetItem(ret, 0, p0o);
	if (r < 0) Py_FatalError("Index 1 out of bounds in a 3-element tuple?");
	r = PyTuple_SetItem(ret, 1, p1o);
	if (r < 0) Py_FatalError("Index 2 out of bounds in a 3-element tuple?");
	return ret;
}

static PyObject *Simulator_copy_amplitudes(PyObject *o, PyObject *arg) {
	SimulatorObject *self = (SimulatorObject*)o;
	Py_buffer buf;
	int r = PyObject_GetBuffer(arg, &buf, PyBUF_WRITABLE | PyBUF_FORMAT | PyBUF_ND);
	if (r < 0) return NULL;
	if (buf.readonly) {
		PyBuffer_Release(&buf);
		PyErr_SetString(PyExc_TypeError, "Expected mutable buffer");
		return NULL;
	}
	if (!PyBuffer_IsContiguous(&buf, 'C')) {
		PyBuffer_Release(&buf);
		PyErr_SetString(PyExc_TypeError, "Expected C-contiguous buffer");
		return NULL;
	}
	if (!buf.format) {
		Py_FatalError("Requested buffer with PyBUF_FORMAT but format not set");
	}

	Py_ssize_t n;
	if (!strcmp(buf.format, "d") || !strcmp(buf.format, "@d") || !strcmp(buf.format, "=d")) {
		if (buf.itemsize != sizeof(double)) {
			Py_FatalError("Buffer of doubles has the wrong item size");
		}
		switch (buf.ndim) {
			case 1:
				n = buf.shape[0];
				if (n % 2 == 1 || n <= 0) {
					PyBuffer_Release(&buf);
					PyErr_SetString(PyExc_TypeError, "Expected even number of elements");
					return NULL;
				}
				n >>= 1;
				break;
			case 2:
				n = buf.shape[0];
				if (buf.shape[1] != 2 || n <= 0) {
					PyBuffer_Release(&buf);
					PyErr_SetString(PyExc_TypeError, "Expected n-by-2 array");
					return NULL;
				}
				break;
			default:
				PyBuffer_Release(&buf);
				PyErr_SetString(PyExc_TypeError, "Expected one or two dimensions");
				return NULL;
		}
	} else if (!strcmp(buf.format, "D") || !strcmp(buf.format, "Zd")) {
		if (buf.itemsize != sizeof(QBlazeComplex)) {
			Py_FatalError("Buffer of complex doubles has the wrong item size");
		}
		if (buf.ndim != 1) {
			PyBuffer_Release(&buf);
			PyErr_SetString(PyExc_TypeError, "Buffer of complex doubles must be one-dimensional");
			return NULL;
		}
		n = buf.shape[0];
	} else {
		PyBuffer_Release(&buf);
		PyErr_Format(PyExc_TypeError, "Expected buffer of doubles, got %s", buf.format);
		return NULL;
	}
	DO_SLOW(r, qblaze_copy_amplitudes, buf.buf, n);
	PyBuffer_Release(&buf);
	if(r < 0) return NULL;
	Py_RETURN_NONE;
}

#define meth_NOARGS2(name, impl) {#name, impl, METH_NOARGS, NULL}
#define meth_NOARGS(name) meth_NOARGS2(name, &Simulator_##name)
#define meth_O(name) {#name, Simulator_##name, METH_O, NULL}
#define meth_FASTCALL(name) {#name, (PyCFunction)(void(*)(void))(1 ? &Simulator_##name : (_PyCFunctionFast)0), METH_FASTCALL, NULL}

static PyMethodDef Simulator_methods[] = {
	{"max_qubit_count", &Simulator_max_qubit_count, METH_CLASS | METH_NOARGS, NULL},

	meth_NOARGS(flush),
	meth_NOARGS(dump),
	meth_NOARGS(clone),
	meth_NOARGS2(__copy__, &Simulator_clone),
	meth_NOARGS2(__deepcopy__, &Simulator_clone),

	meth_O(x),
	meth_O(y),
	meth_O(z),
	meth_O(h),
	meth_O(s),
	meth_O(sdg),
	meth_O(t),
	meth_O(tdg),

	meth_FASTCALL(u3),
	meth_FASTCALL(rx),
	meth_FASTCALL(ry),
	meth_FASTCALL(rz),

	meth_FASTCALL(cx),
	meth_FASTCALL(ccx),
	meth_FASTCALL(mcx),

	meth_FASTCALL(swap),
	meth_FASTCALL(cswap),
	meth_FASTCALL(mcswap),

	meth_FASTCALL(cz),
	meth_FASTCALL(mcphase),

	meth_FASTCALL(measure),
	meth_FASTCALL(measure_ext),
	meth_O(qubit_probs),

	meth_O(copy_amplitudes),

	meth_NOARGS(_perf),

	{0},
};

static PyType_Slot Simulator_slots[] = {
	{Py_tp_new, &Simulator_new},
	{Py_tp_dealloc, &Simulator_dealloc},
	{Py_tp_methods, &Simulator_methods},
	{Py_tp_iter, &Simulator_iter},
	{0},
};

static PyType_Spec Simulator_spec = {
	.name = "qblaze.Simulator",
	.basicsize = sizeof(struct SimulatorObject),
	.flags = Py_TPFLAGS_DEFAULT | Py_TPFLAGS_IMMUTABLETYPE,
	.slots = Simulator_slots,
};

static void Iterator_dealloc(PyObject *self_obj) {
	IteratorObject *self = (IteratorObject*)self_obj;
	SimulatorObject *so = self->sim_obj;
	if (so) {
		assert(so->iter_obj == self);
		Simulator_invalidate_iterator(so, self);
	}
	PyObject_Del(&self->ob_base);
}

static PyObject *Iterator_iter(PyObject *self_obj) {
	return Py_NewRef(self_obj);
}

static PyObject *Iterator_next(PyObject *self_obj) {
	IteratorObject *self = (IteratorObject*)self_obj;
	struct QBlazeIterator *iter = self->iter;
	if (!iter) {
		PyErr_SetString(PyExc_RuntimeError, "Iterator invalidated");
		return NULL;
	}
	int r = qblaze_iter_next(iter);
	if (r == 0) {
		return NULL;
	}

	PyObject *ret = PyTuple_New(2), *o;
	if (!ret) return ret;

	size_t bytes = (iter->qubit_count + 7) / 8;
#if (defined(Py_LIMITED_API) ? Py_LIMITED_API >= 0x030e0000 : PY_VERSION_HEX >= 0x030d0000)
	// Added in 3.13, limited API in 3.14
	o = PyLong_FromUnsignedNativeBytes(iter->bitmap, bytes, Py_ASNATIVEBYTES_LITTLE_ENDIAN);
#else
	o = PyObject_CallMethod((PyObject*)&PyLong_Type, "from_bytes", "y#s", iter->bitmap, bytes, "little");
#endif
	if (!o) goto fail;
	r = PyTuple_SetItem(ret, 0, o);
	if (r < 0) Py_FatalError("Index 0 out of bounds in a 2-element tuple?");

	o = PyComplex_FromDoubles(iter->amplitude.real, iter->amplitude.imag);
	if (!o) goto fail;
	r = PyTuple_SetItem(ret, 1, o);
	if (r < 0) Py_FatalError("Index 1 out of bounds in a 2-element tuple?");

	return ret;

fail:
	Py_DECREF(ret);
	return NULL;
}

static PyType_Slot Iterator_slots[] = {
	{Py_tp_dealloc, &Iterator_dealloc},
	{Py_tp_iter, &Iterator_iter},
	{Py_tp_iternext, &Iterator_next},
	{0},
};

static PyType_Spec Iterator_spec = {
	.name = "qblaze.Iterator",
	.basicsize = sizeof(struct IteratorObject),
	.flags = Py_TPFLAGS_DEFAULT | Py_TPFLAGS_DISALLOW_INSTANTIATION | Py_TPFLAGS_IMMUTABLETYPE,
	.slots = Iterator_slots,
};

static int mod_exec(PyObject *mod) {
	ModState *st = PyModule_GetState(mod);
#define X(name) \
	st->id_##name = PyUnicode_InternFromString(#name); \
	if (!st->id_##name) return -1;
	MOD_IDENTS
#undef X

	st->s_version = PyUnicode_InternFromString(MOD_VERSION);
	if (!st->s_version) return -1;

	PyObject *mod_dict = PyModule_GetDict(mod);
	if (!mod_dict) return -1;

	int r = PyDict_SetItem(mod_dict, st->id___version__, st->s_version);
	if(r < 0) return r;

	PyTypeObject *ty = (PyTypeObject*)PyType_FromModuleAndSpec(mod, &Simulator_spec, NULL);
	if (!ty) return -1;
	st->ty_Simulator = ty;
	r = PyModule_AddType(mod, ty);
	if (r < 0) return r;

	ty = (PyTypeObject*)PyType_FromModuleAndSpec(mod, &Iterator_spec, NULL);
	if (!ty) return -1;
	st->ty_Iterator = ty;
	return PyModule_AddType(mod, ty);
}

static void mod_free(void *stv) {
	ModState *st = stv;
#define X(name, opt, parser) Py_CLEAR(st->id_##name);
	SIMULATOR_OPTS
#undef X
	Py_CLEAR(st->s_version);
	Py_CLEAR(st->ty_Iterator);
	Py_CLEAR(st->ty_Simulator);
}

extern PyObject *PyInit_qblaze(void);
PyObject *PyInit_qblaze(void) {
	return PyModuleDef_Init(&mod_def);
}
