@echo off
rem Run a command with the Visual Studio x64 build environment and Rust on PATH.
rem Usage: scripts\with-msvc.cmd cargo test --workspace
setlocal
set "VSROOT=C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools"
set "VCVER=14.44.35207"
set "SDKVER=10.0.26100.0"
set "PATH=%VSROOT%\VC\Tools\MSVC\%VCVER%\bin\Hostx64\x64;C:\Program Files (x86)\Windows Kits\10\bin\%SDKVER%\x64;%USERPROFILE%\.cargo\bin;%PATH%"
set "LIB=%VSROOT%\VC\Tools\MSVC\%VCVER%\lib\x64;C:\Program Files (x86)\Windows Kits\10\Lib\%SDKVER%\um\x64;C:\Program Files (x86)\Windows Kits\10\Lib\%SDKVER%\ucrt\x64"
set "INCLUDE=%VSROOT%\VC\Tools\MSVC\%VCVER%\include;C:\Program Files (x86)\Windows Kits\10\Include\%SDKVER%\um;C:\Program Files (x86)\Windows Kits\10\Include\%SDKVER%\shared;C:\Program Files (x86)\Windows Kits\10\Include\%SDKVER%\ucrt;C:\Program Files (x86)\Windows Kits\10\Include\%SDKVER%\winrt"
%*
