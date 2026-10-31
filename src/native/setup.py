import sys
from pathlib import Path

import numpy as np
from Cython.Build import cythonize
from setuptools import Extension, setup

native_dir = Path(__file__).resolve().parent

if sys.platform == "win32":
    extra_compile_args = ["/std:c++20", "/O2", "/EHsc", "/DSMARTPOT_EXPORTS"]
    extra_link_args = []
else:
    extra_compile_args = ["-std=c++20", "-O3", "-fPIC", "-march=x86-64", "-DSMARTPOT_EXPORTS"]
    extra_link_args = []

extensions = [
    Extension(
        name="smartpot_cy_core",
        sources=[
            str(native_dir / "cy_bridge.pyx"),
            str(native_dir / "src" / "smartpot_core.cpp"),
        ],
        include_dirs=[
            np.get_include(),
            str(native_dir / "include"),
        ],
        language="c++",
        extra_compile_args=extra_compile_args,
        extra_link_args=extra_link_args,
    )
]

setup(
    name="smartpot_cy_core",
    ext_modules=cythonize(
        extensions,
        compiler_directives={
            "language_level": "3",
            "boundscheck": False,
            "wraparound": False,
            "cdivision": True,
        },
    ),
)
