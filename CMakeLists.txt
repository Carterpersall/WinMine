cmake_minimum_required(VERSION 3.20)
project(winmine LANGUAGES C RC)

set(WINMINE_SOURCES
    winmine.c
    grafix.c
    pref.c
    rtns.c
    sound.c
    util.c
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
    advapi32
    gdi32
        comctl32
)

if (MSVC)
    target_compile_options(winmine PRIVATE /W3 /WX)
    target_link_options(winmine PRIVATE /STACK:0x100000,0x4000 /MANIFEST:NO)
endif()
