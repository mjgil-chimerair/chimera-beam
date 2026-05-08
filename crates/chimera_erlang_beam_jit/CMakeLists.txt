cmake_minimum_required(VERSION 3.16)
project(chimera_jit CXX)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# JIT Compiler sources
add_library(chimera_jit STATIC
    src/jit.cpp
    src/ir.cpp
    src/codegen.cpp
)

target_include_directories(chimera_jit PUBLIC
    ${CMAKE_CURRENT_SOURCE_DIR}/include
)

# Export symbols for Rust FFI
target_compile_definitions(chimera_jit PUBLIC Jit_EXPORTS)