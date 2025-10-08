import json
import os
import subprocess


CARGO = os.getenv('CARGO', 'cargo')


cargo_meta = json.loads(subprocess.check_output([
    CARGO, 'metadata', '--no-deps', '--format-version=1'
]))
[cargo_meta] = cargo_meta['packages']

version = cargo_meta['version']
