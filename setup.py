import json
import os
import platform
import subprocess
import sys
import sysconfig

from setuptools import setup, Extension
from setuptools.command.build_ext import build_ext


CARGO = os.getenv('CARGO', 'cargo')


cargo_meta = json.loads(subprocess.check_output([
    CARGO, 'metadata', '--no-deps', '--format-version=1'
]))
[cargo_meta] = cargo_meta['packages']

LIB_NAME = cargo_meta['name']
LIB_VERSION = cargo_meta['version']


class BuildExt(build_ext):
    def build_extension(self, ext):
        rust_dir = os.path.join(os.path.abspath(self.build_temp), 'rust-build')
        os.makedirs(rust_dir, exist_ok=True)

        cargo_cmd = [
            CARGO, 'rustc',
            '--crate-type=staticlib',
            '--profile=release',
            '--message-format=json',
            '--target-dir=' + rust_dir,
        ]

        # Work around https://github.com/rust-lang/rust/issues/104707
        match platform.system():
            case 'Linux' | 'Android' | 'FreeBSD' | 'OpenBSD':
                ext.extra_link_args.append('-Wl,--exclude-libs,ALL')
            case 'Darwin':
                ext.extra_link_args.append('-Wl,-exported_symbol,_PyInit_' + LIB_NAME)
            case 'Windows':
                # FIXME use `rustc --print native-static-libs`
                # (blocked on https://github.com/rust-lang/cargo/issues/9357)
                ext.libraries.extend(('kernel32', 'advapi32', 'ntdll', 'userenv', 'ws2_32', 'dbghelp'))
                # Fix cross-compiling for 32-bit Python on 64-bit Windows
                if platform.machine() in ('x86', 'AMD64') and platform.architecture()[0] == '32bit':
                    cargo_cmd.append('--target=i686-pc-windows-msvc')

        if platform.system() == 'Linux':
            ext.extra_compile_args.extend((
                '-Wall',
                '-Wextra',
                '-Wstrict-prototypes',
                '-Wmissing-declarations',
                '-Wmissing-prototypes',
            ))
            if os.environ.get('BUILD_WERROR'):
                ext.extra_compile_args.append('-Werror')

        status = subprocess.check_output(cargo_cmd).decode("utf-8")

        found = []
        for line in status.splitlines():
            data = json.loads(line)
            if data['reason'] != 'compiler-artifact':
                continue
            if data['target']['name'] != LIB_NAME:
                continue
            found.append(data)

        match found:
            case []:
                raise RuntimeError('Failed to find build output')
            case [data]:
                pass
            case _:
                raise RuntimeError('Found multiple build outputs?')

        if data['target']['kind'] != ['staticlib'] or data['target']['crate_types'] != ['staticlib']:
            raise RuntimeError('Build output not a staticlib')
        [lib] = data['filenames']
        ext.extra_objects.append(lib)
        ext.depends.extend(ext.extra_objects)

        return super().build_extension(ext)


macros = [
    ('MOD_VERSION', f'"{LIB_VERSION}"'), # The version is only [0-9a-z.-]*
]

py_limited_api = None
if sys.version_info >= (3, 14) and sysconfig.get_config_var('Py_GIL_DISABLED') in (0, None):
    macros.append(('Py_LIMITED_API', f'0x030E0000'))
    py_limited_api = 'cp314'

setup(
    name = LIB_NAME,
    version = LIB_VERSION,
    description = cargo_meta.get('description'),
    package_dir = {'': 'python'},
    packages = [LIB_NAME],
    ext_modules = [Extension(
        name = LIB_NAME + '.__init__',
        sources = ['python/mod.c'],
        include_dirs = ['include'],
        define_macros = macros,
        py_limited_api = py_limited_api,
    )],
    cmdclass={'build_ext': BuildExt},
    options={'bdist_wheel': {'py_limited_api': py_limited_api}},
)
