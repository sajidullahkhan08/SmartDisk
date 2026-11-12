@echo off
rem Build helper: initializes the MSVC x64 environment, then runs cargo.
call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat" >nul
if errorlevel 1 (
    echo Failed to initialize MSVC environment.
    exit /b 1
)
cargo %*
