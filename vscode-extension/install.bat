@echo off
echo Installing HTBasic VS Code extension...
set EXT_DIR=%USERPROFILE%\.vscode\extensions\htbasic.htbasic-0.1.0
if not exist "%EXT_DIR%" mkdir "%EXT_DIR%"
xcopy /E /Y "%~dp0*" "%EXT_DIR%\" >nul
echo Done! Restart VS Code to activate.
echo.
echo Or run: code --install-extension htbasic-0.1.0.vsix
