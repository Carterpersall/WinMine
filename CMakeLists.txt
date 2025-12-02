cmake_minimum_required(VERSION 3.20)
project(winmine LANGUAGES C RC)

option(WINMINE_ENABLE_RUST "Build the Rust port scaffolding" ON)

set(WINMINE_SOURCES
    winmine.c
    rtns.c
    res.rc
)

add_executable(winmine WIN32 ${WINMINE_SOURCES})

target_include_directories(winmine PRIVATE ${CMAKE_CURRENT_SOURCE_DIR})

target_compile_definitions(winmine
    PRIVATE
        WIN32
        _WINDOWS
        _UNICODE
        UNICODE
        WIN32_LEAN_AND_MEAN
        NOMINMAX
        _CRT_SECURE_NO_WARNINGS
)

target_link_libraries(winmine
    PRIVATE
        shell32
        winmm
        htmlhelp
        userenv
        ws2_32
        ntdll
    advapi32
    gdi32
        comctl32
)

if (WINMINE_ENABLE_RUST)
    find_program(CARGO_COMMAND cargo)
    if (NOT CARGO_COMMAND)
        message(WARNING "Cargo not found; disabling WINMINE_ENABLE_RUST")
        set(WINMINE_ENABLE_RUST OFF CACHE BOOL "Build the Rust port scaffolding" FORCE)
    endif()
endif()

if (WINMINE_ENABLE_RUST)
    set(RUST_MANIFEST_PATH ${CMAKE_CURRENT_SOURCE_DIR}/rust/Cargo.toml)
    set(RUST_TARGET_DIR ${CMAKE_BINARY_DIR}/cargo)
    set(RUST_HELPER_SCRIPT ${CMAKE_CURRENT_SOURCE_DIR}/cmake/BuildRust.cmake)

    add_custom_target(winmine_rust_build ALL
        COMMAND ${CMAKE_COMMAND}
            -DCARGO_COMMAND:FILEPATH=${CARGO_COMMAND}
            -DRUST_MANIFEST_PATH:FILEPATH=${RUST_MANIFEST_PATH}
            -DRUST_TARGET_DIR:PATH=${RUST_TARGET_DIR}
            -DRUST_WORKING_DIR:PATH=${CMAKE_CURRENT_SOURCE_DIR}/rust
            -DBUILD_CONFIG=$<CONFIG>
            -P ${RUST_HELPER_SCRIPT}
        WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}/rust
        COMMENT "Building Rust support library"
        VERBATIM
    )

    add_library(winmine_rust STATIC IMPORTED GLOBAL)

    set(RUST_LIB_DEBUG ${RUST_TARGET_DIR}/debug/winmine_rust.lib)
    set(RUST_LIB_RELEASE ${RUST_TARGET_DIR}/release/winmine_rust.lib)

    set_target_properties(winmine_rust PROPERTIES
        IMPORTED_LOCATION_DEBUG ${RUST_LIB_DEBUG}
        IMPORTED_LOCATION_RELWITHDEBINFO ${RUST_LIB_RELEASE}
        IMPORTED_LOCATION_RELEASE ${RUST_LIB_RELEASE}
        IMPORTED_LOCATION_MINSIZEREL ${RUST_LIB_RELEASE}
    )

    add_dependencies(winmine_rust winmine_rust_build)
    add_dependencies(winmine winmine_rust_build)
    target_link_libraries(winmine PRIVATE winmine_rust)
endif()

if (MSVC)
    target_compile_options(winmine PRIVATE /W3 /WX)
    target_link_options(winmine PRIVATE /STACK:0x100000,0x4000 /MANIFEST:NO)
endif()
