if(NOT DEFINED CARGO_COMMAND)
    message(FATAL_ERROR "CARGO_COMMAND is not defined")
endif()

if(NOT DEFINED RUST_MANIFEST_PATH)
    message(FATAL_ERROR "RUST_MANIFEST_PATH is not defined")
endif()

if(NOT DEFINED RUST_TARGET_DIR)
    message(FATAL_ERROR "RUST_TARGET_DIR is not defined")
endif()

if(NOT DEFINED RUST_WORKING_DIR)
    message(FATAL_ERROR "RUST_WORKING_DIR is not defined")
endif()

if(NOT DEFINED OUTPUT_DIRECTORY)
    message(FATAL_ERROR "OUTPUT_DIRECTORY is not defined")
endif()

set(RUST_RELEASE_FLAG)
if(NOT BUILD_CONFIG STREQUAL "Debug")
    set(RUST_RELEASE_FLAG --release)
endif()

execute_process(
    COMMAND ${CARGO_COMMAND} build --manifest-path ${RUST_MANIFEST_PATH} --target-dir ${RUST_TARGET_DIR} ${RUST_RELEASE_FLAG}
    WORKING_DIRECTORY ${RUST_WORKING_DIR}
    RESULT_VARIABLE CARGO_RESULT
)

if(NOT CARGO_RESULT EQUAL 0)
    message(FATAL_ERROR "Cargo build failed with exit code ${CARGO_RESULT}")
endif()

set(RUST_PROFILE_DIR debug)
if(NOT BUILD_CONFIG STREQUAL "Debug")
    set(RUST_PROFILE_DIR release)
endif()

set(RUST_EXE ${RUST_TARGET_DIR}/${RUST_PROFILE_DIR}/winmine.exe)
if(NOT EXISTS ${RUST_EXE})
    message(FATAL_ERROR "Rust build succeeded but ${RUST_EXE} was not found")
endif()

set(CONFIG_OUTPUT_DIR ${OUTPUT_DIRECTORY})
if(BUILD_CONFIG)
    set(CONFIG_OUTPUT_DIR ${OUTPUT_DIRECTORY}/${BUILD_CONFIG})
endif()

file(MAKE_DIRECTORY ${CONFIG_OUTPUT_DIR})
file(COPY ${RUST_EXE} DESTINATION ${CONFIG_OUTPUT_DIR})

set(RUST_PDB ${RUST_TARGET_DIR}/${RUST_PROFILE_DIR}/winmine.pdb)
if(EXISTS ${RUST_PDB})
    file(COPY ${RUST_PDB} DESTINATION ${CONFIG_OUTPUT_DIR})
endif()
