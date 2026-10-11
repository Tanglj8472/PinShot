import os, sys
from .main import main

rc = main()
# PySide6 can segfault during interpreter teardown on some X11 setups; skip it.
sys.stdout.flush(); sys.stderr.flush()
os._exit(rc or 0)
