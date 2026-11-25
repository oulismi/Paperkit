# Captures the website's screenshots from the real app (Windows).
#
#   1. cd app; npm run tauri build -- --no-bundle
#   2. node scripts/fetch-model.mjs            (so Redact shows AI findings too)
#   3. powershell -ExecutionPolicy Bypass -File scripts/screenshots.ps1
#
# Each screen is opened in demo mode (--demo=<screen>) with the sample files in
# tests/fixtures, then only the app window is captured (its own pixels, no
# desktop) into site/screens/<screen>.png. Needs a screen of at least 1280x800.
param(
    [string]$Exe = (Join-Path $PSScriptRoot "..\target\release\paperkit.exe"),
    [string]$Out = (Join-Path $PSScriptRoot "..\site\screens"),
    # Multiplies the waits; use 2-3 with a slower debug build.
    [double]$WaitScale = 1
)
$ErrorActionPreference = "Stop"
$root = Resolve-Path (Join-Path $PSScriptRoot "..")
$fixtures = Join-Path $root "tests\fixtures"
$env:PAPERKIT_MODEL_DIR = Join-Path $root "models\gliner_multi-v2.1"
New-Item -ItemType Directory -Force $Out | Out-Null

Add-Type -AssemblyName System.Drawing
Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
public static class Win {
    [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
    [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int cmd);
    // DWMWA_EXTENDED_FRAME_BOUNDS = 9: the visible window, without the invisible resize border.
    [DllImport("dwmapi.dll")] public static extern int DwmGetWindowAttribute(IntPtr h, int attr, out RECT r, int size);
}
"@
[Win]::SetProcessDPIAware() | Out-Null

# screen name -> seconds to let it finish loading (scans, OCR, AI, thumbnails)
$shots = [ordered]@{ home = 4; organize = 8; redact = 20; convert = 5; edit = 8 }

foreach ($screen in $shots.Keys) {
    Write-Host "capturing $screen"
    # Quoted: the folder path may contain spaces.
    $p = Start-Process -FilePath $Exe -ArgumentList "--demo=$screen", "`"--demo-files=$fixtures`"" -PassThru
    try {
        # Wait for the real app window (early on, the main handle can be a tiny helper window).
        $r = New-Object Win+RECT
        $size = [Runtime.InteropServices.Marshal]::SizeOf($r)
        $deadline = (Get-Date).AddSeconds(40)
        do {
            Start-Sleep -Milliseconds 400; $p.Refresh()
            if ($p.MainWindowHandle -ne 0) { [Win]::DwmGetWindowAttribute($p.MainWindowHandle, 9, [ref]$r, $size) | Out-Null }
        } while (($r.Right - $r.Left) -lt 400 -and (Get-Date) -lt $deadline)
        if (($r.Right - $r.Left) -lt 400) { throw "no window for $screen" }
        Start-Sleep -Seconds ($shots[$screen] * $WaitScale)
        [Win]::ShowWindow($p.MainWindowHandle, 9) | Out-Null   # restore
        [Win]::SetForegroundWindow($p.MainWindowHandle) | Out-Null
        Start-Sleep -Milliseconds 2000   # let the window finish painting in front
        $r = New-Object Win+RECT
        [Win]::DwmGetWindowAttribute($p.MainWindowHandle, 9, [ref]$r, [Runtime.InteropServices.Marshal]::SizeOf($r)) | Out-Null
        $w = $r.Right - $r.Left; $h = $r.Bottom - $r.Top
        $bmp = New-Object Drawing.Bitmap $w, $h
        $g = [Drawing.Graphics]::FromImage($bmp)
        $g.CopyFromScreen($r.Left, $r.Top, 0, 0, $bmp.Size)
        $file = Join-Path $Out "$screen.png"
        $bmp.Save($file, [Drawing.Imaging.ImageFormat]::Png)
        $g.Dispose(); $bmp.Dispose()
        Write-Host "  saved $file ($w x $h)"
    }
    finally {
        if (-not $p.HasExited) { Stop-Process -Id $p.Id -Force }
    }
}
