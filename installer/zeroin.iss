; ZeroIn installer (Inno Setup 6). Built in CI: see .github/workflows/release.yml.
#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#ifndef ModelFile
  #define ModelFile "Qwen3-1.7B-Q4_K_M.gguf"
#endif

[Setup]
AppId={{6F0B6D35-3C8E-4F7B-9A3E-2E7C1D0B5A11}
AppName=ZeroIn
AppVersion={#AppVersion}
AppVerName=ZeroIn {#AppVersion}
AppPublisher=Rozan Abid
AppPublisherURL=https://github.com/Lukecinate/zeroin
AppSupportURL=https://github.com/Lukecinate/zeroin/issues
DefaultDirName={autopf}\ZeroIn
DefaultGroupName=ZeroIn
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
LicenseFile=..\LICENSE
SetupIconFile=..\assets\brand\zeroin.ico
UninstallDisplayIcon={app}\zeroin.exe
OutputDir=..\out
OutputBaseFilename=ZeroIn-Setup-{#AppVersion}-x64
Compression=lzma2/fast
SolidCompression=no
WizardStyle=modern

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "..\dist\zeroin.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\dist\*.dll"; DestDir: "{app}"; Flags: ignoreversion skipifsourcedoesntexist
Source: "..\dist\LICENSE"; DestDir: "{app}"
Source: "..\dist\README.md"; DestDir: "{app}"
Source: "..\dist\CHANGELOG.md"; DestDir: "{app}"
Source: "..\dist\THIRD_PARTY_NOTICES.md"; DestDir: "{app}"
Source: "..\dist\licenses\*"; DestDir: "{app}\licenses"
; The model is already compressed; storing it avoids minutes of pointless LZMA work.
Source: "..\dist\models\{#ModelFile}"; DestDir: "{app}\models"; Flags: nocompression ignoreversion

[Icons]
Name: "{autoprograms}\ZeroIn"; Filename: "{app}\zeroin.exe"
Name: "{autodesktop}\ZeroIn"; Filename: "{app}\zeroin.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\zeroin.exe"; Description: "{cm:LaunchProgram,ZeroIn}"; Flags: nowait postinstall skipifsilent
