@echo off
rem Build PinShot.exe on Windows (cmd). Requires Python 3.10+ ("py" launcher or PYTHON=path\python.exe).
setlocal
cd /d "%~dp0"
if "%PYTHON%"=="" (set "PYEXE=py -3") else (set "PYEXE=%PYTHON%")
if not exist .venv-win (%PYEXE% -m venv .venv-win || goto :err)
set "VPY=.venv-win\Scripts\python.exe"
%VPY% -m pip install --upgrade pip || goto :err
%VPY% -m pip install -r requirements-windows.txt || goto :err
%VPY% tools\make_icon.py || goto :err
%VPY% tests\test_platform.py || goto :err
%VPY% -m PyInstaller --noconfirm --clean pinshot.spec || goto :err
set "PINSHOT_VERSION_OUT=%CD%\ver.txt"
start /wait "" dist\PinShot\PinShot.exe --version
type ver.txt
echo Built: %CD%\dist\PinShot\PinShot.exe
exit /b 0
:err
echo Build failed.
exit /b 1
