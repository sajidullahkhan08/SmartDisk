@echo off
rem Print the DLL imports of a built executable (developer diagnostic).
call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat" >nul
if errorlevel 1 exit /b 1
dumpbin /dependents "%~1"
