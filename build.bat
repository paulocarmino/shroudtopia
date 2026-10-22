@echo OFF 
echo Starting Build...
call "%VCINSTALLDIR%\..\Common7\Tools\VSDevCmd.bat"
echo.
RD /S /Q build
mkdir build
mkdir build\x64
MSBuild "shroudtopia\shroudtopia.sln" /t:Build /p:Configuration=Release /p:OutDir="..\build\x64" /p:Platform=x64
rem Minimap (third-party/Enshrouded-minimap, a pinned submodule): its DLL and its assets/ make mods\minimap_mod
MSBuild "third-party\Enshrouded-minimap\minimap_mod.vcxproj" /t:Build /p:Configuration=Release /p:Platform=x64 /p:PlatformToolset=v143 /p:OutDir="%~dp0build\x64\mods\minimap_mod\\"
xcopy /E /I /Y /Q "third-party\Enshrouded-minimap\assets" "build\x64\mods\minimap_mod" >NUL
echo. 
echo All builds completed.
echo.
erase build\x64\*.pdb
erase build\x64\*.exp
erase build\x64\*.lib
erase build\x64\mods\minimap_mod\*.pdb build\x64\mods\minimap_mod\*.exp build\x64\mods\minimap_mod\*.lib
echo. 
echo Cleaned up.