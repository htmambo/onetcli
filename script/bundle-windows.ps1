[CmdletBinding()]
param(
    [string]$Target = "x86_64-pc-windows-msvc",
    [string]$Version = "",
    [string]$OutputRoot = "",
    [switch]$SkipBuild
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Resolve-ExecutablePath {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Name,
        [string[]]$Fallbacks = @()
    )

    $command = Get-Command $Name -ErrorAction SilentlyContinue
    if ($command) {
        return $command.Source
    }

    foreach ($candidate in $Fallbacks) {
        if ([string]::IsNullOrWhiteSpace($candidate)) {
            continue
        }

        if (Test-Path -LiteralPath $candidate) {
            return (Resolve-Path -LiteralPath $candidate).Path
        }
    }

    throw "未找到 $Name，请先安装对应工具并确认路径可用。"
}

function Resolve-VsDevCmdPath {
    $programFilesX86 = [Environment]::GetFolderPath("ProgramFilesX86")
    $programFiles = [Environment]::GetFolderPath("ProgramFiles")
    $vswhere = Join-Path $programFilesX86 "Microsoft Visual Studio\Installer\vswhere.exe"
    if (Test-Path -LiteralPath $vswhere) {
        $installPath = & $vswhere -products * -requires Microsoft.VisualStudio.Workload.VCTools -property installationPath
        if ($LASTEXITCODE -eq 0 -and -not [string]::IsNullOrWhiteSpace($installPath)) {
            $candidate = Join-Path $installPath "Common7\Tools\VsDevCmd.bat"
            if (Test-Path -LiteralPath $candidate) {
                return $candidate
            }
        }
    }

    $fallbacks = @(
        (Join-Path $programFilesX86 "Microsoft Visual Studio\2022\BuildTools\Common7\Tools\VsDevCmd.bat"),
        (Join-Path $programFiles "Microsoft Visual Studio\2022\BuildTools\Common7\Tools\VsDevCmd.bat"),
        (Join-Path $programFilesX86 "Microsoft Visual Studio\2022\Community\Common7\Tools\VsDevCmd.bat"),
        (Join-Path $programFiles "Microsoft Visual Studio\2022\Community\Common7\Tools\VsDevCmd.bat")
    )

    foreach ($candidate in $fallbacks) {
        if (Test-Path -LiteralPath $candidate) {
            return $candidate
        }
    }

    throw "未找到 VsDevCmd.bat，请先运行 script/install-window.ps1 或安装 Visual Studio BuildTools。"
}

function Get-AppVersion {
    param(
        [Parameter(Mandatory = $true)]
        [string]$CargoTomlPath,
        [string]$OverrideVersion = ""
    )

    if (-not [string]::IsNullOrWhiteSpace($OverrideVersion)) {
        return $OverrideVersion.Trim()
    }

    $versionLine = Get-Content -LiteralPath $CargoTomlPath -Encoding UTF8 |
        Select-String -Pattern '^version\s*=\s*"([^"]+)"' |
        Select-Object -First 1

    if (-not $versionLine) {
        throw "无法从 $CargoTomlPath 解析版本号。"
    }

    return $versionLine.Matches[0].Groups[1].Value
}

function Invoke-CmdOrThrow {
    param(
        [Parameter(Mandatory = $true)]
        [string]$CommandLine,
        [Parameter(Mandatory = $true)]
        [string]$WorkingDirectory
    )

    Push-Location $WorkingDirectory
    try {
        & cmd.exe /d /c $CommandLine
        if ($LASTEXITCODE -ne 0) {
            throw "命令执行失败，退出码: $LASTEXITCODE"
        }
    }
    finally {
        Pop-Location
    }
}

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$projectDir = Split-Path -Parent $scriptDir
$mainCargoToml = Join-Path $projectDir "main\Cargo.toml"
$versionValue = Get-AppVersion -CargoTomlPath $mainCargoToml -OverrideVersion $Version
$versionTag = if ($versionValue.StartsWith("v")) { $versionValue } else { "v$versionValue" }

$cargoExe = Resolve-ExecutablePath -Name "cargo.exe" -Fallbacks @(
    "$env:USERPROFILE\.cargo\bin\cargo.exe"
)
$cmakeExe = Resolve-ExecutablePath -Name "cmake.exe" -Fallbacks @(
    "C:\Program Files\CMake\bin\cmake.exe",
    "C:\Program Files (x86)\CMake\bin\cmake.exe"
)
$vsDevCmd = Resolve-VsDevCmdPath

$cargoDir = Split-Path -Parent $cargoExe
$cmakeDir = Split-Path -Parent $cmakeExe

if ([string]::IsNullOrWhiteSpace($OutputRoot)) {
    $distRoot = Join-Path $projectDir "target\dist\windows-x64"
}
else {
    if ([System.IO.Path]::IsPathRooted($OutputRoot)) {
        $distRoot = $OutputRoot
    }
    else {
        $distRoot = Join-Path $projectDir $OutputRoot
    }
}

$bundleName = "OnetCli-windows-x64"
$artifactBaseName = "onetcli-$versionTag-windows-x64"
$stageDir = Join-Path $distRoot $bundleName
$zipPath = Join-Path $distRoot "$artifactBaseName.zip"
$portableExePath = Join-Path $distRoot "$artifactBaseName.exe"
$hashFilePath = Join-Path $distRoot "SHA256SUMS.txt"
$binaryPath = Join-Path $projectDir "target\$Target\release\onetcli.exe"

Write-Host "开始打包 Windows x64 应用..."
Write-Host "项目目录: $projectDir"
Write-Host "目标平台: $Target"
Write-Host "版本号: $versionTag"
Write-Host "输出目录: $distRoot"

New-Item -ItemType Directory -Path $distRoot -Force | Out-Null

if (-not $SkipBuild) {
    $buildCommand = ('"{0}" -arch=x64 && set "PATH={1};{2};%PATH%" && "{3}" build --release -p main --target {4}' -f `
        $vsDevCmd, $cargoDir, $cmakeDir, $cargoExe, $Target)

    Write-Host "执行发布构建..."
    Invoke-CmdOrThrow -CommandLine $buildCommand -WorkingDirectory $projectDir
}
else {
    Write-Host "已跳过构建，直接使用现有二进制。"
}

if (-not (Test-Path -LiteralPath $binaryPath)) {
    throw "未找到发布二进制: $binaryPath"
}

if (Test-Path -LiteralPath $stageDir) {
    Remove-Item -LiteralPath $stageDir -Recurse -Force
}
New-Item -ItemType Directory -Path $stageDir -Force | Out-Null

Copy-Item -LiteralPath $binaryPath -Destination (Join-Path $stageDir "onetcli.exe") -Force
Copy-Item -LiteralPath $binaryPath -Destination $portableExePath -Force

foreach ($file in @("LICENSE-APACHE", "ONETCLI_LICENSE", "README.md", "README_CN.md")) {
    $source = Join-Path $projectDir $file
    if (Test-Path -LiteralPath $source) {
        Copy-Item -LiteralPath $source -Destination (Join-Path $stageDir $file) -Force
    }
}

# Copy bundled themes
$themeSourceDir = Join-Path $projectDir "themes"
$themeDestDir = Join-Path $stageDir "themes"
if (Test-Path -LiteralPath $themeSourceDir) {
    New-Item -ItemType Directory -Path $themeDestDir -Force | Out-Null
    $jsonFiles = @(Get-ChildItem -LiteralPath $themeSourceDir -Filter "*.json" -File)
    $jsoncFiles = @(Get-ChildItem -LiteralPath $themeSourceDir -Filter "*.jsonc" -File)
    $allThemeFiles = $jsonFiles + $jsoncFiles
    foreach ($themeFile in $allThemeFiles) {
        Copy-Item -LiteralPath $themeFile.FullName -Destination $themeDestDir -Force
    }
    Write-Host "已复制 $($allThemeFiles.Count) 个主题文件到打包目录"
}
else {
    Write-Host "警告：未找到主题目录 ${themeSourceDir}，跳过主题复制"
}

$buildInfo = @(
    "应用: OnetCli"
    "版本: $versionTag"
    "目标: $Target"
    "生成时间: $(Get-Date -Format 'yyyy-MM-dd HH:mm:ss K')"
    "二进制: onetcli.exe"
) -join [Environment]::NewLine

Set-Content -LiteralPath (Join-Path $stageDir "BUILD_INFO.txt") -Value $buildInfo -Encoding UTF8

if (Test-Path -LiteralPath $zipPath) {
    Remove-Item -LiteralPath $zipPath -Force
}
Add-Type -AssemblyName System.IO.Compression.FileSystem
[System.IO.Compression.ZipFile]::CreateFromDirectory(
    $stageDir,
    $zipPath,
    [System.IO.Compression.CompressionLevel]::Optimal,
    $true
)

$zipHash = (Get-FileHash -LiteralPath $zipPath -Algorithm SHA256).Hash.ToLowerInvariant()
$exeHash = (Get-FileHash -LiteralPath $portableExePath -Algorithm SHA256).Hash.ToLowerInvariant()

$hashLines = @(
    "$zipHash *$(Split-Path -Leaf $zipPath)"
    "$exeHash *$(Split-Path -Leaf $portableExePath)"
)
Set-Content -LiteralPath $hashFilePath -Value $hashLines -Encoding UTF8

Write-Host ""
Write-Host "打包完成，产物如下："
Get-ChildItem -LiteralPath $distRoot | Select-Object Name, Length, LastWriteTime | Format-Table -AutoSize
