#pragma once

// Compatibility shim used by the original WinMine sources to build as either
// a 16-bit (Win16) or 32-bit (Win32) binary. Only the Win32 path is relevant
// for this modernized build, so we provide the handful of helper macros that
// the code expects when compiling for Win32.

#ifndef PORT1632_STUB_H
#define PORT1632_STUB_H

#include <windows.h>
#include <string.h>

#ifdef __cplusplus
extern "C" {
#endif

#ifndef MMain
// The legacy sources declare the entry point through the MMain macro so they
// could share the same signature across 16-bit and 32-bit builds. Map it to
// the Win32 WinMain entry point.
#define MMain(hInstance, hPrevInstance, lpCmdLine, nCmdShow)                               \
    int APIENTRY WinMain(HINSTANCE hInstance, HINSTANCE hPrevInstance, LPSTR lpCmdLine, int nCmdShow) \
    {
#endif

// The original header exposed hmemcpy/hmemset as aliases for the Win16 APIs.
// On Win32 these simply route to the CRT helpers, so define them accordingly.
#ifndef hmemcpy
#define hmemcpy memcpy
#endif

#ifndef hmemset
#define hmemset memset
#endif

// No-op macros that kept Win16 compilers happy but are meaningless today.
#ifndef EXPENTRY
#define EXPENTRY WINAPI
#endif

#ifndef WINAPI16
#define WINAPI16 CALLBACK
#endif

#ifndef max
#define max(a,b)            (((a) > (b)) ? (a) : (b))
#endif

#ifndef min
#define min(a,b)            (((a) < (b)) ? (a) : (b))
#endif

__inline int MMoveTo(HDC hdc, int x, int y)
{
    POINT pt;
    return MoveToEx(hdc, x, y, &pt);
}

#ifdef __cplusplus
}
#endif

#endif // PORT1632_STUB_H
