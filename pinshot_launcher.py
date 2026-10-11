"""PyInstaller entry point (PyInstaller needs a script, not `-m pinshot`)."""
import os, sys
from pinshot.main import main

if __name__ == "__main__":
    import multiprocessing
    multiprocessing.freeze_support()
    rc = main()
    sys.stdout and sys.stdout.flush(); sys.stderr and sys.stderr.flush()
    os._exit(rc or 0)      # skip PySide6 teardown (segfault-prone on some setups)
