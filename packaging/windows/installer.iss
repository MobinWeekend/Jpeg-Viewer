#define AppVersion GetEnv("JPEG_VIEWER_VERSION")
#define TargetArch GetEnv("JPEG_VIEWER_ARCH")
#define BinaryDir GetEnv("JPEG_VIEWER_BINARY_DIR")

#if AppVersion == ""
  #error JPEG_VIEWER_VERSION environment variable is required
#endif
#if TargetArch == ""
  #error JPEG_VIEWER_ARCH environment variable is required
#endif
#if BinaryDir == ""
  #error JPEG_VIEWER_BINARY_DIR environment variable is required
#endif
#if GetEnv("JPEG_VIEWER_FILE_VERSION") == ""
  #error JPEG_VIEWER_FILE_VERSION environment variable is required
#endif

#if TargetArch == "x64"
  #define AllowedArchitectures "x64compatible and not arm64"
  #define Install64BitMode "x64compatible and not arm64"
#elif TargetArch == "ARM64"
  #define AllowedArchitectures "arm64"
  #define Install64BitMode "arm64"
#else
  #error Unsupported JPEG_VIEWER_ARCH; expected x64 or ARM64
#endif

[Setup]
AppId={{A6E7B5D1-2D8A-4B17-9D56-7F7AE5C61C4D}}
AppName=JPEG Viewer
AppVersion={#AppVersion}
AppVerName=JPEG Viewer {#AppVersion}
AppPublisher=MobinWeekend
VersionInfoVersion={#GetEnv("JPEG_VIEWER_FILE_VERSION")}
VersionInfoCompany=MobinWeekend
VersionInfoDescription=JPEG Viewer Setup
VersionInfoProductName=JPEG Viewer
VersionInfoProductVersion={#GetEnv("JPEG_VIEWER_FILE_VERSION")}
VersionInfoCopyright=Copyright (C) MobinWeekend
AppPublisherURL=https://github.com/MobinWeekend/Jpeg-Viewer
AppSupportURL=https://github.com/MobinWeekend/Jpeg-Viewer/issues
AppUpdatesURL=https://github.com/MobinWeekend/Jpeg-Viewer/releases

; Enforce per-user installation. No UAC elevation or administrator password.
PrivilegesRequired=lowest
DefaultDirName={localappdata}\Programs\JPEG Viewer
DefaultGroupName=JPEG Viewer
DisableProgramGroupPage=yes
UsePreviousAppDir=yes

ArchitecturesAllowed={#AllowedArchitectures}
ArchitecturesInstallIn64BitMode={#Install64BitMode}

SetupIconFile=..\..\assets\icons\icon.ico
UninstallDisplayIcon={app}\jpeg_viewer.exe
UninstallDisplayName=JPEG Viewer
LicenseFile=..\..\LICENSE
OutputDir=..\..\dist\windows-installers
OutputBaseFilename=JPEG-Viewer-Setup-{#TargetArch}

WizardStyle=modern dynamic
Compression=lzma2
SolidCompression=yes
SetupLogging=yes
CloseApplications=yes
CloseApplicationsFilter=jpeg_viewer.exe
RestartApplications=no
ChangesAssociations=yes

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; GroupDescription: "Additional shortcuts:"; Flags: unchecked
Name: "fileassoc"; Description: "Add JPEG Viewer to Open with for supported images and archives"; GroupDescription: "File integration:"; Flags: unchecked

[Files]
Source: "{#BinaryDir}\JPEG-Viewer-Windows-{#TargetArch}.exe"; DestDir: "{app}"; DestName: "jpeg_viewer.exe"; Flags: ignoreversion
Source: "..\..\README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\QUICK_START.md"; DestDir: "{app}"; DestName: "GETTING_STARTED.md"; Flags: ignoreversion
Source: "..\..\LICENSE"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\JPEG Viewer"; Filename: "{app}\jpeg_viewer.exe"; WorkingDir: "{userdocs}"
Name: "{autodesktop}\JPEG Viewer"; Filename: "{app}\jpeg_viewer.exe"; Tasks: desktopicon

[Registry]
; Register an Open With ProgID without changing the user's default application.
Root: HKCU; Subkey: "Software\Classes\JPEGViewer.Image"; ValueType: string; ValueName: ""; ValueData: "JPEG Viewer image"; Flags: uninsdeletekey; Tasks: fileassoc
Root: HKCU; Subkey: "Software\Classes\JPEGViewer.Image\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\jpeg_viewer.exe,0"; Tasks: fileassoc
Root: HKCU; Subkey: "Software\Classes\JPEGViewer.Image\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\jpeg_viewer.exe"" ""%1"""; Tasks: fileassoc

; A queue is a distinct document type, not an image.
Root: HKCU; Subkey: "Software\Classes\JPEGViewer.Queue"; ValueType: string; ValueName: ""; ValueData: "JPEG Viewer queue"; Flags: uninsdeletekey; Tasks: fileassoc
Root: HKCU; Subkey: "Software\Classes\JPEGViewer.Queue\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\jpeg_viewer.exe,0"; Tasks: fileassoc
Root: HKCU; Subkey: "Software\Classes\JPEGViewer.Queue\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\jpeg_viewer.exe"" ""%1"""; Tasks: fileassoc
Root: HKCU; Subkey: "Software\Classes\.jvq\OpenWithProgids"; ValueType: none; ValueName: "JPEGViewer.Queue"; Flags: uninsdeletevalue; Tasks: fileassoc

; Supported raster, vector, HDR, portable and camera RAW formats.
Root: HKCU; Subkey: "Software\Classes\.jpg\OpenWithProgids"; ValueType: none; ValueName: "JPEGViewer.Image"; Flags: uninsdeletevalue; Tasks: fileassoc
Root: HKCU; Subkey: "Software\Classes\.jpeg\OpenWithProgids"; ValueType: none; ValueName: "JPEGViewer.Image"; Flags: uninsdeletevalue; Tasks: fileassoc
Root: HKCU; Subkey: "Software\Classes\.jpe\OpenWithProgids"; ValueType: none; ValueName: "JPEGViewer.Image"; Flags: uninsdeletevalue; Tasks: fileassoc
Root: HKCU; Subkey: "Software\Classes\.png\OpenWithProgids"; ValueType: none; ValueName: "JPEGViewer.Image"; Flags: uninsdeletevalue; Tasks: fileassoc
Root: HKCU; Subkey: "Software\Classes\.gif\OpenWithProgids"; ValueType: none; ValueName: "JPEGViewer.Image"; Flags: uninsdeletevalue; Tasks: fileassoc
Root: HKCU; Subkey: "Software\Classes\.bmp\OpenWithProgids"; ValueType: none; ValueName: "JPEGViewer.Image"; Flags: uninsdeletevalue; Tasks: fileassoc
Root: HKCU; Subkey: "Software\Classes\.webp\OpenWithProgids"; ValueType: none; ValueName: "JPEGViewer.Image"; Flags: uninsdeletevalue; Tasks: fileassoc
Root: HKCU; Subkey: "Software\Classes\.tif\OpenWithProgids"; ValueType: none; ValueName: "JPEGViewer.Image"; Flags: uninsdeletevalue; Tasks: fileassoc
Root: HKCU; Subkey: "Software\Classes\.tiff\OpenWithProgids"; ValueType: none; ValueName: "JPEGViewer.Image"; Flags: uninsdeletevalue; Tasks: fileassoc
Root: HKCU; Subkey: "Software\Classes\.avif\OpenWithProgids"; ValueType: none; ValueName: "JPEGViewer.Image"; Flags: uninsdeletevalue; Tasks: fileassoc
Root: HKCU; Subkey: "Software\Classes\.heic\OpenWithProgids"; ValueType: none; ValueName: "JPEGViewer.Image"; Flags: uninsdeletevalue; Tasks: fileassoc
Root: HKCU; Subkey: "Software\Classes\.heif\OpenWithProgids"; ValueType: none; ValueName: "JPEGViewer.Image"; Flags: uninsdeletevalue; Tasks: fileassoc
Root: HKCU; Subkey: "Software\Classes\.jxl\OpenWithProgids"; ValueType: none; ValueName: "JPEGViewer.Image"; Flags: uninsdeletevalue; Tasks: fileassoc
Root: HKCU; Subkey: "Software\Classes\.svg\OpenWithProgids"; ValueType: none; ValueName: "JPEGViewer.Image"; Flags: uninsdeletevalue; Tasks: fileassoc
Root: HKCU; Subkey: "Software\Classes\.ico\OpenWithProgids"; ValueType: none; ValueName: "JPEGViewer.Image"; Flags: uninsdeletevalue; Tasks: fileassoc
Root: HKCU; Subkey: "Software\Classes\.dds\OpenWithProgids"; ValueType: none; ValueName: "JPEGViewer.Image"; Flags: uninsdeletevalue; Tasks: fileassoc
Root: HKCU; Subkey: "Software\Classes\.exr\OpenWithProgids"; ValueType: none; ValueName: "JPEGViewer.Image"; Flags: uninsdeletevalue; Tasks: fileassoc
Root: HKCU; Subkey: "Software\Classes\.hdr\OpenWithProgids"; ValueType: none; ValueName: "JPEGViewer.Image"; Flags: uninsdeletevalue; Tasks: fileassoc
Root: HKCU; Subkey: "Software\Classes\.tga\OpenWithProgids"; ValueType: none; ValueName: "JPEGViewer.Image"; Flags: uninsdeletevalue; Tasks: fileassoc
Root: HKCU; Subkey: "Software\Classes\.qoi\OpenWithProgids"; ValueType: none; ValueName: "JPEGViewer.Image"; Flags: uninsdeletevalue; Tasks: fileassoc
Root: HKCU; Subkey: "Software\Classes\.ff\OpenWithProgids"; ValueType: none; ValueName: "JPEGViewer.Image"; Flags: uninsdeletevalue; Tasks: fileassoc
Root: HKCU; Subkey: "Software\Classes\.pnm\OpenWithProgids"; ValueType: none; ValueName: "JPEGViewer.Image"; Flags: uninsdeletevalue; Tasks: fileassoc
Root: HKCU; Subkey: "Software\Classes\.cr2\OpenWithProgids"; ValueType: none; ValueName: "JPEGViewer.Image"; Flags: uninsdeletevalue; Tasks: fileassoc
Root: HKCU; Subkey: "Software\Classes\.cr3\OpenWithProgids"; ValueType: none; ValueName: "JPEGViewer.Image"; Flags: uninsdeletevalue; Tasks: fileassoc
Root: HKCU; Subkey: "Software\Classes\.nef\OpenWithProgids"; ValueType: none; ValueName: "JPEGViewer.Image"; Flags: uninsdeletevalue; Tasks: fileassoc
Root: HKCU; Subkey: "Software\Classes\.nrw\OpenWithProgids"; ValueType: none; ValueName: "JPEGViewer.Image"; Flags: uninsdeletevalue; Tasks: fileassoc
Root: HKCU; Subkey: "Software\Classes\.arw\OpenWithProgids"; ValueType: none; ValueName: "JPEGViewer.Image"; Flags: uninsdeletevalue; Tasks: fileassoc
Root: HKCU; Subkey: "Software\Classes\.dng\OpenWithProgids"; ValueType: none; ValueName: "JPEGViewer.Image"; Flags: uninsdeletevalue; Tasks: fileassoc
Root: HKCU; Subkey: "Software\Classes\.raf\OpenWithProgids"; ValueType: none; ValueName: "JPEGViewer.Image"; Flags: uninsdeletevalue; Tasks: fileassoc
Root: HKCU; Subkey: "Software\Classes\.orf\OpenWithProgids"; ValueType: none; ValueName: "JPEGViewer.Image"; Flags: uninsdeletevalue; Tasks: fileassoc
Root: HKCU; Subkey: "Software\Classes\.rw2\OpenWithProgids"; ValueType: none; ValueName: "JPEGViewer.Image"; Flags: uninsdeletevalue; Tasks: fileassoc
Root: HKCU; Subkey: "Software\Classes\.pef\OpenWithProgids"; ValueType: none; ValueName: "JPEGViewer.Image"; Flags: uninsdeletevalue; Tasks: fileassoc
Root: HKCU; Subkey: "Software\Classes\.srw\OpenWithProgids"; ValueType: none; ValueName: "JPEGViewer.Image"; Flags: uninsdeletevalue; Tasks: fileassoc
Root: HKCU; Subkey: "Software\Classes\.3fr\OpenWithProgids"; ValueType: none; ValueName: "JPEGViewer.Image"; Flags: uninsdeletevalue; Tasks: fileassoc
Root: HKCU; Subkey: "Software\Classes\.fff\OpenWithProgids"; ValueType: none; ValueName: "JPEGViewer.Image"; Flags: uninsdeletevalue; Tasks: fileassoc
Root: HKCU; Subkey: "Software\Classes\.iiq\OpenWithProgids"; ValueType: none; ValueName: "JPEGViewer.Image"; Flags: uninsdeletevalue; Tasks: fileassoc
Root: HKCU; Subkey: "Software\Classes\.kdc\OpenWithProgids"; ValueType: none; ValueName: "JPEGViewer.Image"; Flags: uninsdeletevalue; Tasks: fileassoc
Root: HKCU; Subkey: "Software\Classes\.mos\OpenWithProgids"; ValueType: none; ValueName: "JPEGViewer.Image"; Flags: uninsdeletevalue; Tasks: fileassoc
Root: HKCU; Subkey: "Software\Classes\.mrw\OpenWithProgids"; ValueType: none; ValueName: "JPEGViewer.Image"; Flags: uninsdeletevalue; Tasks: fileassoc
Root: HKCU; Subkey: "Software\Classes\.rwl\OpenWithProgids"; ValueType: none; ValueName: "JPEGViewer.Image"; Flags: uninsdeletevalue; Tasks: fileassoc
Root: HKCU; Subkey: "Software\Classes\.x3f\OpenWithProgids"; ValueType: none; ValueName: "JPEGViewer.Image"; Flags: uninsdeletevalue; Tasks: fileassoc

; Archives supported by the application.
Root: HKCU; Subkey: "Software\Classes\.zip\OpenWithProgids"; ValueType: none; ValueName: "JPEGViewer.Image"; Flags: uninsdeletevalue; Tasks: fileassoc
Root: HKCU; Subkey: "Software\Classes\.7z\OpenWithProgids"; ValueType: none; ValueName: "JPEGViewer.Image"; Flags: uninsdeletevalue; Tasks: fileassoc
Root: HKCU; Subkey: "Software\Classes\.rar\OpenWithProgids"; ValueType: none; ValueName: "JPEGViewer.Image"; Flags: uninsdeletevalue; Tasks: fileassoc
