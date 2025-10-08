Install
#######

While qblaze is written in Rust, we primarily intend it to be used via its
Python and C bindings.

For Python, we recommend installing qblaze via the pip package manager
(ensure your Python version is ≥3.10):

.. code-block:: console

    $ pip install qblaze


Building from source
====================

The source code is available on `GitHub <https://github.com/insait-institute/qblaze>`_.

Prerequisites:

- `Python <https://www.python.org/>`_, version 3.10 or newer,
  with the ability to build extension modules (C toolchain, headers, etc.).
  On various Linux distributions this may correspond to :code:`python`, :code:`python3-dev`, or :code:`python3-devel`.
  On Windows you will need either Visual Studio or Build Tools for Visual Studio.

- `Rust <https://www.rust-lang.org/>`_, version 1.82 or newer.
  Note that the same C toolchain must used by Rust and Python.
  For example, `the official python.org Python distribution for Windows <https://www.python.org/downloads/windows/>`_
  uses the MSVC toolchain, so the Rust target should be :code:`x86_64-pc-windows-msvc`, not :code:`x86_64-pc-windows-gnu`.

The setuptools script :file:`setup.py` takes care of building the Python extension module:

.. code-block:: console

    $ pip install .

The C bindings can be built with `cargo-c <https://github.com/lu-zero/cargo-c>`_:

.. code-block:: console

    $ cargo cinstall
