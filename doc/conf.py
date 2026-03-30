import os
import subprocess
import sys
import tomllib


with open('../Cargo.toml', 'rb') as f:
    cargo_meta = tomllib.load(f)

project = cargo_meta['package']['name']
version = cargo_meta['package']['version']

html_context = {
    'display_github': True,
    'github_user': 'insait-institute',
    'github_repo': project,
    'github_version': 'master',
    'conf_py_path': '/doc/',
}

html_theme_options = {
    'logo_only': True,
}
html_favicon = '_static/favicon-192x192.png'

smv_tag_whitelist = r'^v[0-9]'
smv_branch_whitelist = r'^master$'
smv_released_pattern = r'^refs/tags/v[0-9]'
smv_outputdir_format = '{ref.name}'

html_theme = 'sphinx_rtd_theme'
html_static_path = ['_static']
html_logo = '_static/logo.svg'

templates_path = [
    '_templates',
]

extensions = [
    'sphinx.ext.autodoc',
    'sphinx.ext.mathjax',
    'sphinx.ext.mathjax',
    'sphinx_multiversion',
    'sphinx_c_autodoc',
    'sphinx_inline_tabs',
]

autodoc_default_options = {
    'member-order': 'bysource',
    'members': True,
    'undoc-members': True,
}

c_autodoc_roots = ['..']
try:
    _clang_libdir = os.environ['QBLAZE_SPHINX_AUTODOC_CLANG_LIBDIR']
except KeyError:
    pass
else:
    import clang.cindex
    clang.cindex.Config.set_library_path(_clang_libdir)


def _html_page_context(app, pagename, templatename, context, doctree):
    app.add_css_file('theme-custom.css')
    if pagename == 'index':
        app.add_js_file('plotly-3.1.0.min.js')
        app.add_css_file('plotly-custom.css')


def _env_updated(app, env):
    env.master_doctree = env.get_doctree('toc')
    toc_path = env.doc2path('toc')
    for docname in env.found_docs:
        if docname != 'toc':
            env.note_dependency(toc_path, docname=docname)


def setup(app):
    tmpdir = f'{app.doctreedir}/mod-build'
    sys.path.insert(0, tmpdir)
    env = {**os.environ}
    env.setdefault('RUSTFLAGS', f'-C opt-level=1 -C lto=no -C codegen-units={2*os.process_cpu_count()}')
    subprocess.check_call(
        [sys.executable, '-P', 'setup.py', 'build', '--build-lib', tmpdir],
        cwd = '..',
        env = env,
    )

    app.connect('html-page-context', _html_page_context)
    app.connect('env-updated', _env_updated)
