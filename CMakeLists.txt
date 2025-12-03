cmake_minimum_required(VERSION 3.20)
project(winmine LANGUAGES NONE)

option(WINMINE_ENABLE_RUST "Build the Rust WinMine executable via Cargo" ON)

if (NOT WINMINE_ENABLE_RUST)
    message(FATAL_ERROR "Non-Rust builds are no longer supported.")
endif()

find_program(CARGO_COMMAND cargo)
if (NOT CARGO_COMMAND)
    message(FATAL_ERROR "Cargo not found; install Rust to build WinMine.")
endif()

set(RUST_MANIFEST_PATH ${CMAKE_CURRENT_SOURCE_DIR}/rust/Cargo.toml)
set(RUST_TARGET_DIR ${CMAKE_BINARY_DIR}/cargo)
set(RUST_HELPER_SCRIPT ${CMAKE_CURRENT_SOURCE_DIR}/cmake/BuildRust.cmake)

add_custom_target(winmine ALL
    COMMAND ${CMAKE_COMMAND}
        -DCARGO_COMMAND:FILEPATH=${CARGO_COMMAND}
        -DRUST_MANIFEST_PATH:FILEPATH=${RUST_MANIFEST_PATH}
        -DRUST_TARGET_DIR:PATH=${RUST_TARGET_DIR}
        -DRUST_WORKING_DIR:PATH=${CMAKE_CURRENT_SOURCE_DIR}/rust
        -DOUTPUT_DIRECTORY:PATH=${CMAKE_CURRENT_BINARY_DIR}
        -DBUILD_CONFIG=$<CONFIG>
        -P ${RUST_HELPER_SCRIPT}
    WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}/rust
    COMMENT "Building WinMine via Cargo"
    VERBATIM
)
