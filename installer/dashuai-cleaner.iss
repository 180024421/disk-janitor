; 大帅清理器 — Inno Setup 安装脚本
; 由 一键安装包.cmd 调用 ISCC 编译

#ifndef MyAppVersion
  #define MyAppVersion "0.9.0"
#endif

#define MyAppName "大帅清理器"
#define MyAppNameEn "Dashuai Cleaner"
#define MyAppPublisher "lidashuai"
#define MyAppURL "https://github.com/180024421/disk-janitor"
#define MyAppExeName "大帅清理器.exe"
#define MyAppId "180024421.DashuaiCleaner"

[Setup]
AppId={{#MyAppId}}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppVerName={#MyAppName} {#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL={#MyAppURL}
AppSupportURL={#MyAppURL}
DefaultDirName={autopf}\{#MyAppName}
DefaultGroupName={#MyAppName}
DisableProgramGroupPage=yes
LicenseFile=
OutputDir=..\release
OutputBaseFilename=大帅清理器-Setup-{#MyAppVersion}
SetupIconFile=
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
PrivilegesRequired=admin
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
UninstallDisplayName={#MyAppName}
VersionInfoVersion={#MyAppVersion}.0
VersionInfoProductName={#MyAppName}
VersionInfoCompany={#MyAppPublisher}
VersionInfoDescription={#MyAppName} 安装程序
ChineseSimplified=yes

[Languages]
Name: "chinesesimplified"; MessagesFile: "compiler:Languages\ChineseSimplified.isl"
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "创建桌面快捷方式"; GroupDescription: "附加图标:"; Flags: unchecked
Name: "quicklaunchicon"; Description: "创建快速启动栏图标"; GroupDescription: "附加图标:"; Flags: unchecked; OnlyBelowVersion: 6.1
Name: "explorercontext"; Description: "添加资源管理器右键“用大帅清理器分析”"; GroupDescription: "附加功能:"; Flags: unchecked

[Files]
; 主程序（打包脚本会复制为 大帅清理器.exe）
Source: "..\release\pack\{#MyAppExeName}"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\release\pack\disk-janitor.exe"; DestDir: "{app}"; Flags: ignoreversion
; 资源（赞助码等）
Source: "..\release\pack\resources\*"; DestDir: "{app}\resources"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{group}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"
Name: "{group}\卸载 {#MyAppName}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; Tasks: desktopicon

[Registry]
Root: HKCR; Subkey: "Directory\shell\DashuaiCleaner"; ValueType: string; ValueName: ""; ValueData: "用大帅清理器分析"; Flags: uninsdeletekey; Tasks: explorercontext
Root: HKCR; Subkey: "Directory\shell\DashuaiCleaner"; ValueType: string; ValueName: "Icon"; ValueData: "{app}\{#MyAppExeName}"; Tasks: explorercontext
Root: HKCR; Subkey: "Directory\shell\DashuaiCleaner\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#MyAppExeName}"" --path ""%1"" --scan"; Tasks: explorercontext
Root: HKCR; Subkey: "Directory\Background\shell\DashuaiCleaner"; ValueType: string; ValueName: ""; ValueData: "用大帅清理器分析"; Flags: uninsdeletekey; Tasks: explorercontext
Root: HKCR; Subkey: "Directory\Background\shell\DashuaiCleaner"; ValueType: string; ValueName: "Icon"; ValueData: "{app}\{#MyAppExeName}"; Tasks: explorercontext
Root: HKCR; Subkey: "Directory\Background\shell\DashuaiCleaner\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#MyAppExeName}"" --path ""%V"" --scan"; Tasks: explorercontext
Root: HKCR; Subkey: "Drive\shell\DashuaiCleaner"; ValueType: string; ValueName: ""; ValueData: "用大帅清理器分析"; Flags: uninsdeletekey; Tasks: explorercontext
Root: HKCR; Subkey: "Drive\shell\DashuaiCleaner"; ValueType: string; ValueName: "Icon"; ValueData: "{app}\{#MyAppExeName}"; Tasks: explorercontext
Root: HKCR; Subkey: "Drive\shell\DashuaiCleaner\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#MyAppExeName}"" --path ""%1"" --scan"; Tasks: explorercontext

[Run]
Filename: "{app}\{#MyAppExeName}"; Description: "立即运行 {#MyAppName}"; Flags: nowait postinstall skipifsilent

[Code]
function InitializeSetup(): Boolean;
begin
  Result := True;
end;
