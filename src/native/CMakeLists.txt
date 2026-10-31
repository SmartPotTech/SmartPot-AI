cmake_minimum_required(VERSION 3.20)
project(smartpot_core VERSION 2.0.0 LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_POSITION_INDEPENDENT_CODE ON)

add_library(smartpot_core SHARED
    src/smartpot_core.cpp
)

target_include_directories(smartpot_core PUBLIC
    $<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}/include>
    $<INSTALL_INTERFACE:include>
)

target_compile_definitions(smartpot_core PRIVATE SMARTPOT_EXPORTS)

if(MSVC)
    target_compile_options(smartpot_core PRIVATE /W4 /O2 /permissive-)
else()
    target_compile_options(smartpot_core PRIVATE -Wall -Wextra -Wpedantic -O3 -march=x86-64)
endif()

set_target_properties(smartpot_core PROPERTIES
    PREFIX ""
    OUTPUT_NAME "smartpot_core"
    LIBRARY_OUTPUT_DIRECTORY "${CMAKE_CURRENT_SOURCE_DIR}/../../app/native"
    RUNTIME_OUTPUT_DIRECTORY "${CMAKE_CURRENT_SOURCE_DIR}/../../app/native"
)
