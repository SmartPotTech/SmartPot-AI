CXX ?= g++
CXXFLAGS ?= -O3 -fPIC -std=c++20 -DNDEBUG -Wall -Wextra -Wpedantic -march=x86-64 -Iinclude
LDFLAGS ?= -shared
PYTHON ?= python3

TARGET_DIR = ../../app/native
TARGET = $(TARGET_DIR)/libsmartpot_core.so
SRCS = src/smartpot_core.cpp

all: $(TARGET) cython

$(TARGET): $(SRCS)
	@mkdir -p $(TARGET_DIR)
	$(CXX) $(CXXFLAGS) $(LDFLAGS) -o $@ $(SRCS)
	@echo "Built C++ native acceleration core: $@"

cython:
	@mkdir -p $(TARGET_DIR)
	-$(PYTHON) setup.py build_ext --build-lib $(TARGET_DIR)

clean:
	rm -f $(TARGET) $(TARGET_DIR)/smartpot_cy_core*.so cy_bridge.cpp
	rm -rf build

.PHONY: all clean cython
