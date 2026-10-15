"""Resolve a registered class (e.g. a UI controller) to its factory, constructor, vtable and
slot functions. Kept as a shortcut; the implementation lives in types.py.

Usage: python3 tools/triage/controller.py ClassName [slots]
  (same as: python3 tools/triage/types.py class ClassName --slots N)
"""
import os
import subprocess
import sys

if __name__ == '__main__':
    if len(sys.argv) < 2:
        print(__doc__)
        sys.exit(1)
    args = [sys.executable, os.path.join(os.path.dirname(os.path.abspath(__file__)), 'types.py'),
            'class', sys.argv[1]]
    if len(sys.argv) > 2:
        args += ['--slots', sys.argv[2]]
    sys.exit(subprocess.call(args))
