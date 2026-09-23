; LanMusic Windows 安装包脚本（Inno Setup 6）
; 一键打包: powershell -ExecutionPolicy Bypass -File installer\build.ps1
;          （等价于: npm run desktop → 本脚本编译）
; 输出: installer\output\LanMusic_<版本>_x64-setup.exe

#define MyAppName "LanMusic"
#define MyAppExeName "lanmusic.exe"
; AUMID：与 tauri.conf.json 的 identifier 一致（lib.rs::ensure_app_user_model_id
; 设置的进程级 AUMID 与此匹配，SMTC 媒体浮层/任务栏据此解析出应用名「LanMusic」）
#define MyAppAumid "com.lanmusic.desktop"
; 完整版本号取自编译产物的文件版本（Windows 文件版本是四段，如 0.5.5.0），用于向导展示
; 与「应用和功能」里的版本号。文件名另用下面的短版本（需构建脚本传入）。
#define MyAppVersion GetFileVersion("..\src-tauri\target\release\lanmusic.exe")
; 短版本（三段，如 0.5.5）：**只用于安装包文件名**。
; 应用内更新按 `LanMusic_<三段版本>_x64-setup.exe` 拼接资产地址
; （src-tauri/src/updater.rs::asset_urls），文件名多出第四段 ".0" 会让 HEAD 探测 404、
; 更新退化为「前往下载链接」。构建脚本（installer/build.ps1 与 CI）统一以
;   ISCC /DMyAppVersionShort=<tauri.conf.json 里的 version>
; 传入，保证与 App 内比较用的版本、tag 名三者一致。
; 未传入时（直接手跑 ISCC）回退为完整文件版本——此时产物名会带第四段，仅供本地试编。
#ifndef MyAppVersionShort
  #define MyAppVersionShort MyAppVersion
#endif

[Setup]
; 固定 GUID：升级识别键。一旦发布不要再改动，否则旧版本无法被覆盖升级
AppId={{6E7F1A42-9C3B-4D58-A1E2-3F4B5C6D7E8F}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppVerName={#MyAppName} {#MyAppVersion}
AppPublisher={#MyAppName}
DefaultDirName={autopf}\{#MyAppName}
DisableProgramGroupPage=yes
OutputDir=output
OutputBaseFilename=LanMusic_{#MyAppVersionShort}_x64-setup
SetupIconFile=..\src-tauri\icons\icon.ico
UninstallDisplayIcon={app}\{#MyAppExeName}
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
; 与 Tauri NSIS 默认一致：优先按当前用户安装（{autopf} 解析为 %LOCALAPPDATA%\Programs），
; 用户可在向导中改为为所有用户安装
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
ShowLanguageDialog=no

[Languages]
; 语言文件随仓库提供（见 installer/languages/）：Inno Setup 官方安装包不含简中翻译
; （社区维护），若引用 compiler:Languages\... 会在未手动安装该文件的环境（如 CI）编译失败
Name: "chinesesimplified"; MessagesFile: "languages\ChineseSimplified.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"

[Files]
; 覆盖前先结束正在运行的实例（LanMusic 关闭默认驻留托盘，必须强杀）
Source: "..\src-tauri\target\release\lanmusic.exe"; DestDir: "{app}"; Flags: ignoreversion; BeforeInstall: KillRunningApp

[Registry]
; SMTC 媒体浮层/系统通知按 AUMID 解析应用显示名：不注册的话系统媒体浮层显示
; 「未知应用」。应用每次启动也会自写同一键（兜底便携运行），此处负责卸载清理。
Root: HKCU; Subkey: "Software\Classes\AppUserModelId\com.lanmusic.desktop"; ValueType: string; ValueName: "DisplayName"; ValueData: "{#MyAppName}"; Flags: uninsdeletekey

[Icons]
; AppUserModelID 写进 .lnk 属性：系统媒体浮层（SMTC）按「进程 AUMID ↔ 快捷方式
; AUMID」解析应用名，快捷方式不带它就一直显示「未知应用」（注册表 AppUserModelId
; 键只服务 Toast 通知，SMTC 不读）。需 Inno Setup 6.3+（CI 为最新版，满足）。
Name: "{autoprograms}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; AppUserModelID: "{#MyAppAumid}"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; Tasks: desktopicon; AppUserModelID: "{#MyAppAumid}"

[Run]
; 应用内更新调用安装器时带 /LAUNCH=1（静默安装，装完自动拉起新版）；
; 其他静默安装（无该参数）不启动应用，保持部署脚本的中立性
Filename: "{app}\{#MyAppExeName}"; Description: "{cm:LaunchProgram,{#MyAppName}}"; Flags: nowait postinstall; Check: ShouldLaunchApp

[Code]
procedure KillRunningApp;
var
  R: Integer;
begin
  Exec(ExpandConstant('{sys}\taskkill.exe'), '/f /im {#MyAppExeName}', '', SW_HIDE, ewWaitUntilTerminated, R);
end;

{ 询问是否一并删除用户数据（曲库/歌单/设置/背景图）。默认选中「否」 }
function AskRemoveUserData: Boolean;
begin
  Result := MsgBox(
    '是否一并删除 LanMusic 的用户数据？' + #13#10 + #13#10 +
    '包括曲库数据库、歌单、偏好设置与背景图（位于 %APPDATA%\com.lanmusic.desktop 与 %LOCALAPPDATA%\com.lanmusic.desktop）。' + #13#10 + #13#10 +
    '选择「是」：永久删除这些数据，重新安装后从零开始；' + #13#10 +
    '选择「否」（推荐）：保留数据，重新安装后可直接继续使用。',
    mbConfirmation, MB_YESNO or MB_DEFBUTTON2) = IDYES;
end;

{ 卸载开始时先结束运行中的实例：LanMusic 关闭窗口默认驻留托盘，卸载时进程通常
  仍在运行并占用 lanmusic.exe 的文件句柄 —— Inno 会跳过被占用的文件却仍然提示
  「卸载完成」，留下需要手动删除的残留。这里在卸载向导出现前强杀，
  并留出句柄释放时间，保证后续文件删除干净。
  随后（非静默卸载）询问是否一并清理用户数据，默认保留。 }
function InitializeUninstall(): Boolean;
begin
  KillRunningApp;
  Sleep(800);
  if (not UninstallSilent) and AskRemoveUserData then
  begin
    DelTree(ExpandConstant('{userappdata}\com.lanmusic.desktop'), True, True, True);
    DelTree(ExpandConstant('{localappdata}\com.lanmusic.desktop'), True, True, True);
  end;
  Result := True;
end;

{ 是否执行「启动应用」项：
  - 向导安装（非静默）：显示，由用户勾选；
  - 静默安装：仅当带 /LAUNCH=1（应用内更新流程，见 src-tauri/src/updater.rs）才启动，
    其余静默部署保持不弹应用 }
function ShouldLaunchApp: Boolean;
begin
  Result := (not WizardSilent) or (ExpandConstant('{param:LAUNCH|0}') = '1');
end;

function IsWebView2Installed: Boolean;
var
  Pv: string;
begin
  Result := False;
  if RegQueryStringValue(HKLM64, 'SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}', 'pv', Pv) and (Pv <> '') then
    Result := True
  else if RegQueryStringValue(HKLM32, 'SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}', 'pv', Pv) and (Pv <> '') then
    Result := True
  else if RegQueryStringValue(HKCU, 'SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}', 'pv', Pv) and (Pv <> '') then
    Result := True;
end;

procedure OpenWebView2Download;
var
  R: Integer;
begin
  ShellExec('open', 'https://go.microsoft.com/fwlink/?linkid=2124701', '', '', SW_SHOWNORMAL, ewNoWait, R);
end;

function NextButtonClick(CurPageID: Integer): Boolean;
begin
  Result := True;
  if (CurPageID = wpReady) and (not IsWebView2Installed) then
  begin
    if MsgBox(
        '系统未检测到 Microsoft Edge WebView2 运行时，缺少它 LanMusic 将无法启动。' + #13#10 + #13#10 +
        '是否现在打开官方下载页面？下载安装完成后，再重新运行本安装程序即可。',
        mbConfirmation, MB_YESNO) = IDYES then
      OpenWebView2Download;
    Result := False;
  end;
end;
