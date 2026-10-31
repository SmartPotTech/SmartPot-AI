@echo off
setlocal

set "SCRIPT_DIR=%~dp0"
set "OUT_DIR=%SCRIPT_DIR%..\..\app\native"

if not exist "%OUT_DIR%" mkdir "%OUT_DIR%"

where cl >nul 2>nul
if %errorlevel% neq 0 (
    if exist "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat" (
        call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat"
    ) else if exist "C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Auxiliary\Build\vcvars64.bat" (
        call "C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Auxiliary\Build\vcvars64.bat"
    ) else if exist "C:\Program Files\Microsoft Visual Studio\2022\Professional\VC\Auxiliary\Build\vcvars64.bat" (
        call "C:\Program Files\Microsoft Visual Studio\2022\Professional\VC\Auxiliary\Build\vcvars64.bat"
    ) else if exist "C:\Program Files\Microsoft Visual Studio\2022\Enterprise\VC\Auxiliary\Build\vcvars64.bat" (
        call "C:\Program Files\Microsoft Visual Studio\2022\Enterprise\VC\Auxiliary\Build\vcvars64.bat"
    )
)

cl /std:c++20 /O2 /LD /EHsc /DSMARTPOT_EXPORTS /I"%SCRIPT_DIR%include" "%SCRIPT_DIR%src\smartpot_core.cpp" /Fe"%OUT_DIR%\smartpot_core.dll" /Fo"%OUT_DIR%\smartpot_core.obj"

if %errorlevel% equ 0 (
    echo [OK] Built native C++ core: %OUT_DIR%\smartpot_core.dll
    del "%OUT_DIR%\smartpot_core.exp" "%OUT_DIR%\smartpot_core.lib" "%OUT_DIR%\smartpot_core.obj" 2>nul
) else (
    echo [ERROR] Failed to compile C++ core.
    exit /b 1
)

endlocal
