# Generates production README visual assets in docs/assets/ by compositing real
# Direct2D showcase renders from Mochi.exe onto high-DPI showcase frames.
# Usage: ./tools/make-readme-art.ps1 [-ShotsDir target\showcase-tmp] [-PerformanceOnly]
param(
    [string]$ShotsDir = "target\showcase-tmp",
    [switch]$PerformanceOnly
)
$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.Drawing

$root = Split-Path $PSScriptRoot -Parent
$shots = if ([System.IO.Path]::IsPathRooted($ShotsDir)) { $ShotsDir } else { Join-Path $root $ShotsDir }
$outDir = Join-Path $root "docs\assets"
New-Item -ItemType Directory -Force -Path $outDir | Out-Null
$dot = [char]0x00B7
$emdash = [char]0x2014

function New-RR([float]$x, [float]$y, [float]$w, [float]$h, [float]$r) {
    $p = New-Object System.Drawing.Drawing2D.GraphicsPath
    $r = [Math]::Min($r, [Math]::Min($w, $h) * 0.5)
    $d = $r * 2.0
    if ($d -le 0.5) {
        $p.AddRectangle([System.Drawing.RectangleF]::new($x, $y, $w, $h))
        return $p
    }
    $p.AddArc($x, $y, $d, $d, 180, 90)
    $p.AddArc($x + $w - $d, $y, $d, $d, 270, 90)
    $p.AddArc($x + $w - $d, $y + $h - $d, $d, $d, 0, 90)
    $p.AddArc($x, $y + $h - $d, $d, $d, 90, 90)
    $p.CloseFigure()
    return $p
}

function New-TopRR([float]$x, [float]$y, [float]$w, [float]$h, [float]$r) {
    $p = New-Object System.Drawing.Drawing2D.GraphicsPath
    $r = [Math]::Min($r, [Math]::Min($w, $h) * 0.5)
    $d = $r * 2.0
    $p.AddArc($x, $y, $d, $d, 180, 90)
    $p.AddArc($x + $w - $d, $y, $d, $d, 270, 90)
    $p.AddLine($x + $w, $y + $r, $x + $w, $y + $h)
    $p.AddLine($x + $w, $y + $h, $x, $y + $h)
    $p.CloseFigure()
    return $p
}

function New-Canvas([int]$w, [int]$h) {
    $bmp = New-Object System.Drawing.Bitmap($w, $h, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::HighQuality
    $g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
    $g.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
    $g.TextRenderingHint = [System.Drawing.Text.TextRenderingHint]::AntiAliasGridFit
    return @($bmp, $g)
}

function Draw-Backdrop($g, [int]$w, [int]$h) {
    $rect = [System.Drawing.Rectangle]::new(0, 0, $w, $h)
    $bg = New-Object System.Drawing.Drawing2D.LinearGradientBrush(
        $rect,
        [System.Drawing.Color]::FromArgb(255, 6, 8, 15),
        [System.Drawing.Color]::FromArgb(255, 11, 16, 30),
        90.0
    )
    $g.FillRectangle($bg, $rect)
    $bg.Dispose()

    # Subtle dot grid
    $dotBrush = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(14, 255, 255, 255))
    for ($gx = 24; $gx -lt $w; $gx += 32) {
        for ($gy = 24; $gy -lt $h; $gy += 32) {
            $g.FillEllipse($dotBrush, $gx, $gy, 2, 2)
        }
    }
    $dotBrush.Dispose()
}

function Draw-RadialGlow($g, [float]$cx, [float]$cy, [float]$rx, [float]$ry, [System.Drawing.Color]$col) {
    $gp = New-Object System.Drawing.Drawing2D.GraphicsPath
    $gp.AddEllipse($cx - $rx, $cy - $ry, $rx * 2.0, $ry * 2.0)
    $pgb = New-Object System.Drawing.Drawing2D.PathGradientBrush($gp)
    $pgb.CenterColor = $col
    $pgb.SurroundColors = @([System.Drawing.Color]::FromArgb(0, $col.R, $col.G, $col.B))
    $g.FillPath($pgb, $gp)
    $pgb.Dispose()
    $gp.Dispose()
}

function Draw-ShotAnchored($g, [string]$fileName, [float]$centerX, [float]$topY, [float]$maxW, [float]$maxH) {
    $path = Join-Path $shots $fileName
    if (-not (Test-Path $path)) { throw "Missing showcase shot: $path" }
    $img = [System.Drawing.Image]::FromFile($path)
    try {
        $scale = [Math]::Min($maxW / $img.Width, $maxH / $img.Height)
        $dw = [float]($img.Width * $scale)
        $dh = [float]($img.Height * $scale)
        $dx = [float]($centerX - $dw * 0.5)
        $g.DrawImage($img, [System.Drawing.RectangleF]::new($dx, $topY, $dw, $dh))
        return @($dx, $topY, $dw, $dh)
    } finally {
        $img.Dispose()
    }
}

function Draw-Pill($g, [float]$x, [float]$y, [float]$h, [string]$text, [System.Drawing.Color]$dotCol) {
    $font = New-Object System.Drawing.Font("Segoe UI", 10.5, [System.Drawing.FontStyle]::Bold)
    $sz = $g.MeasureString($text, $font)
    $w = [float]($sz.Width + 38.0)
    $p = New-RR $x $y $w $h ($h * 0.5)
    $fill = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(22, 255, 255, 255))
    $pen = New-Object System.Drawing.Pen([System.Drawing.Color]::FromArgb(42, 255, 255, 255), 1.2)
    $g.FillPath($fill, $p)
    $g.DrawPath($pen, $p)
    $dotB = New-Object System.Drawing.SolidBrush($dotCol)
    $g.FillEllipse($dotB, $x + 13.0, $y + ($h - 8.0) * 0.5, 8.0, 8.0)
    $txtB = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(235, 241, 245, 249))
    $sf = New-Object System.Drawing.StringFormat
    $sf.LineAlignment = [System.Drawing.StringAlignment]::Center
    $g.DrawString($text, $font, $txtB, [System.Drawing.RectangleF]::new($x + 26.0, $y, $w - 32.0, $h), $sf)
    $sf.Dispose(); $txtB.Dispose(); $dotB.Dispose(); $pen.Dispose(); $fill.Dispose(); $p.Dispose(); $font.Dispose()
    return $w
}

function Draw-DesktopCard($g, [float]$x, [float]$y, [float]$w, [float]$h, [float]$r, [float]$bezelH, [System.Drawing.Color]$wallA, [System.Drawing.Color]$wallB) {
    $cardPath = New-RR $x $y $w $h $r
    $cardFill = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(255, 13, 17, 28))
    $g.FillPath($cardFill, $cardPath)
    $cardFill.Dispose()

    $state = $g.Save()
    $g.SetClip($cardPath)

    # Wallpaper area at top
    $wallH = $h - 74.0
    $wallRect = [System.Drawing.RectangleF]::new($x, $y, $w, $wallH)
    $wallBrush = New-Object System.Drawing.Drawing2D.LinearGradientBrush($wallRect, $wallA, $wallB, 35.0)
    $g.FillRectangle($wallBrush, $wallRect)
    $wallBrush.Dispose()

    # Top black bezel so notch concave ears blend seamlessly
    $bezelBrush = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(255, 0, 0, 0))
    $g.FillRectangle($bezelBrush, $x, $y, $w, $bezelH)
    $bezelBrush.Dispose()

    # Subtle divider above caption
    $divPen = New-Object System.Drawing.Pen([System.Drawing.Color]::FromArgb(24, 255, 255, 255), 1.0)
    $g.DrawLine($divPen, $x, $y + $wallH, $x + $w, $y + $wallH)
    $divPen.Dispose()

    $g.Restore($state)

    $borderPen = New-Object System.Drawing.Pen([System.Drawing.Color]::FromArgb(36, 255, 255, 255), 1.2)
    $g.DrawPath($borderPen, $cardPath)
    $borderPen.Dispose()
    $cardPath.Dispose()
}

if (-not $PerformanceOnly) {
# ---------------------------------------------------------------------------
# 1. HERO BANNER (1600 x 880)
# ---------------------------------------------------------------------------
$bmp, $g = New-Canvas 1600 880
Draw-Backdrop $g 1600 880
Draw-RadialGlow $g 320 140 460 260 ([System.Drawing.Color]::FromArgb(42, 138, 180, 255))
Draw-RadialGlow $g 1280 160 460 260 ([System.Drawing.Color]::FromArgb(36, 124, 58, 237))
Draw-RadialGlow $g 800 620 620 280 ([System.Drawing.Color]::FromArgb(32, 56, 189, 248))

# Mochi Emblem (Notch pill with Cozmo Expressive eyes)
$emW = 136.0; $emH = 62.0; $emX = (1600.0 - $emW) * 0.5; $emY = 42.0
Draw-RadialGlow $g 800 ($emY + $emH * 0.5) 140 64 ([System.Drawing.Color]::FromArgb(65, 138, 180, 255))
$emPath = New-RR $emX $emY $emW $emH 22.0
$emFill = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(255, 8, 10, 16))
$emPen = New-Object System.Drawing.Pen([System.Drawing.Color]::FromArgb(95, 138, 180, 255), 1.6)
$g.FillPath($emFill, $emPath)
$g.DrawPath($emPen, $emPath)
$eyeBrush = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(255, 248, 249, 252))
foreach ($sign in @(-1, 1)) {
    $ep = New-RR (800.0 + $sign * 20.0 - 10.5) ($emY + ($emH - 24.0) * 0.5) 21.0 24.0 7.5
    $g.FillPath($eyeBrush, $ep)
    $ep.Dispose()
}
$eyeBrush.Dispose(); $emPen.Dispose(); $emFill.Dispose(); $emPath.Dispose()

# Wordmark & Tagline
$sfCenter = New-Object System.Drawing.StringFormat
$sfCenter.Alignment = [System.Drawing.StringAlignment]::Center
$titleFont = New-Object System.Drawing.Font("Segoe UI", 50.0, [System.Drawing.FontStyle]::Bold)
$subFont = New-Object System.Drawing.Font("Segoe UI", 17.5, [System.Drawing.FontStyle]::Regular)
$whiteB = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(255, 255, 255, 255))
$subB = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(215, 186, 200, 224))
$g.DrawString("Mochi", $titleFont, $whiteB, [System.Drawing.RectangleF]::new(0, 114, 1600, 86), $sfCenter)
$g.DrawString("The Native Windows Notch & AI Agent Companion", $subFont, $subB, [System.Drawing.RectangleF]::new(0, 204, 1600, 36), $sfCenter)
$subB.Dispose(); $whiteB.Dispose(); $subFont.Dispose(); $titleFont.Dispose()

# Tech Pills Row
$pills = @(
    @("Rust 2024", [System.Drawing.Color]::FromArgb(255, 251, 146, 60)),
    @("Direct2D $dot 100 Hz", [System.Drawing.Color]::FromArgb(255, 138, 180, 255)),
    @("WinRT + CoreAudio", [System.Drawing.Color]::FromArgb(255, 56, 189, 248)),
    @("10 Interactive Tabs", [System.Drawing.Color]::FromArgb(255, 52, 211, 153)),
    @("MIT Open Source", [System.Drawing.Color]::FromArgb(255, 167, 139, 250))
)
$pillFont = New-Object System.Drawing.Font("Segoe UI", 10.5, [System.Drawing.FontStyle]::Bold)
$totalPillW = 0.0
foreach ($p in $pills) { $totalPillW += ($g.MeasureString($p[0], $pillFont).Width + 38.0) }
$totalPillW += ($pills.Count - 1) * 12.0
$pillFont.Dispose()
$px = (1600.0 - $totalPillW) * 0.5
foreach ($p in $pills) {
    $pw = Draw-Pill $g $px 256.0 34.0 $p[0] $p[1]
    $px += $pw + 12.0
}

# Desktop Showcase Frame
$fx = 70.0; $fy = 322.0; $fw = 1460.0; $fh = 516.0
$framePath = New-RR $fx $fy $fw $fh 24.0
$state = $g.Save()
$g.SetClip($framePath)
$wallRect = [System.Drawing.RectangleF]::new($fx, $fy, $fw, $fh)
$wallGrad = New-Object System.Drawing.Drawing2D.LinearGradientBrush(
    $wallRect,
    [System.Drawing.Color]::FromArgb(255, 14, 22, 44),
    [System.Drawing.Color]::FromArgb(255, 24, 20, 56),
    30.0
)
$g.FillRectangle($wallGrad, $wallRect)
$wallGrad.Dispose()
Draw-RadialGlow $g ($fx + 320) ($fy + 360) 420 220 ([System.Drawing.Color]::FromArgb(55, 59, 130, 246))
Draw-RadialGlow $g ($fx + 1140) ($fy + 340) 420 220 ([System.Drawing.Color]::FromArgb(50, 139, 92, 246))

# Top bezel bar
$bezelB = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(255, 0, 0, 0))
$g.FillRectangle($bezelB, $fx, $fy, $fw, 14.0)
$bezelB.Dispose()

# Real Direct2D expanded notch + Copilot panel anchored flush to top bezel
$null = Draw-ShotAnchored $g "open-copilot.png" 800.0 ($fy + 12.0) 1380.0 320.0

# 3 Feature Highlight Cards inside bottom of desktop frame
$cards = @(
    @("100 Hz Spring Physics", "DWM-vsync locked Direct2D rendering with 0% idle CPU", [System.Drawing.Color]::FromArgb(255, 138, 180, 255)),
    @("AI Agent Live Bubbles", "Real-time hooks for Copilot, Claude Code, Cursor & Codex", [System.Drawing.Color]::FromArgb(255, 110, 158, 255)),
    @("Local-First & Encrypted", "DPAPI token storage, capture exclusion & zero telemetry", [System.Drawing.Color]::FromArgb(255, 52, 211, 153))
)
$cw = 436.0; $ch = 104.0; $cgap = 24.0; $cx0 = $fx + ($fw - (3 * $cw + 2 * $cgap)) * 0.5; $cy0 = $fy + 382.0
$cTitleF = New-Object System.Drawing.Font("Segoe UI", 13.5, [System.Drawing.FontStyle]::Bold)
$cBodyF = New-Object System.Drawing.Font("Segoe UI", 10.5, [System.Drawing.FontStyle]::Regular)
$cTitleB = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(255, 248, 250, 252))
$cBodyB = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(200, 186, 198, 220))
for ($i = 0; $i -lt 3; $i++) {
    $cx = $cx0 + $i * ($cw + $cgap)
    $cp = New-RR $cx $cy0 $cw $ch 16.0
    $cf = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(175, 10, 13, 22))
    $cpen = New-Object System.Drawing.Pen([System.Drawing.Color]::FromArgb(42, 255, 255, 255), 1.2)
    $g.FillPath($cf, $cp)
    $g.DrawPath($cpen, $cp)
    $accentB = New-Object System.Drawing.SolidBrush($cards[$i][2])
    $g.FillEllipse($accentB, $cx + 18.0, $cy0 + 22.0, 10.0, 10.0)
    $g.DrawString($cards[$i][0], $cTitleF, $cTitleB, [System.Drawing.RectangleF]::new($cx + 36.0, $cy0 + 14.0, $cw - 52.0, 28.0))
    $g.DrawString($cards[$i][1], $cBodyF, $cBodyB, [System.Drawing.RectangleF]::new($cx + 18.0, $cy0 + 46.0, $cw - 36.0, 46.0))
    $accentB.Dispose(); $cpen.Dispose(); $cf.Dispose(); $cp.Dispose()
}
$cBodyB.Dispose(); $cTitleB.Dispose(); $cBodyF.Dispose(); $cTitleF.Dispose()

$g.Restore($state)
$framePen = New-Object System.Drawing.Pen([System.Drawing.Color]::FromArgb(52, 255, 255, 255), 1.6)
$g.DrawPath($framePen, $framePath)
$framePen.Dispose(); $framePath.Dispose(); $sfCenter.Dispose()
$bmp.Save((Join-Path $outDir "hero-banner.png"), [System.Drawing.Imaging.ImageFormat]::Png)
$g.Dispose(); $bmp.Dispose()

# ---------------------------------------------------------------------------
# 2. COMPACT & AMBIENT STATES SHOWCASE (1600 x 608)
# ---------------------------------------------------------------------------
$bmp, $g = New-Canvas 1600 608
Draw-Backdrop $g 1600 608
Draw-RadialGlow $g 400 300 500 260 ([System.Drawing.Color]::FromArgb(28, 138, 180, 255))
Draw-RadialGlow $g 1200 340 500 260 ([System.Drawing.Color]::FromArgb(26, 52, 211, 153))

$hTitleF = New-Object System.Drawing.Font("Segoe UI", 24.0, [System.Drawing.FontStyle]::Bold)
$hSubF = New-Object System.Drawing.Font("Segoe UI", 12.0, [System.Drawing.FontStyle]::Regular)
$hTitleB = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(255, 255, 255, 255))
$hSubB = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(200, 160, 174, 198))
$g.DrawString("Always-On Compact & Ambient States", $hTitleF, $hTitleB, 48.0, 26.0)
$g.DrawString("Adapts dynamically at 100 Hz $emdash from a quiet clock and gaze-tracking visor to live media, focus timers, HUDs, and AI agent bubbles.", $hSubF, $hSubB, 50.0, 70.0)

$compactCards = @(
    @{ Title = "Clock & Ambient Robo Visor"; Desc = "Live clock, cursor-tracking gaze & sleep mode"; Shots = @("closed-clock.png", "closed-sleeping.png"); WallA = [System.Drawing.Color]::FromArgb(255, 18, 26, 48); WallB = [System.Drawing.Color]::FromArgb(255, 28, 24, 58) },
    @{ Title = "Live Media & Equalizer"; Desc = "SMTC album art, synced lyrics & 100 Hz waveform"; Shots = @("closed-media.png"); WallA = [System.Drawing.Color]::FromArgb(255, 22, 24, 56); WallB = [System.Drawing.Color]::FromArgb(255, 42, 26, 74) },
    @{ Title = "Focus & Meeting Bubble"; Desc = "Pomodoro progress ring & upcoming calendar alerts"; Shots = @("closed-focus.png"); WallA = [System.Drawing.Color]::FromArgb(255, 16, 30, 48); WallB = [System.Drawing.Color]::FromArgb(255, 22, 44, 68) },
    @{ Title = "System HUD Overlays"; Desc = "Volume, brightness, power, Caps Lock & privacy dots"; Shots = @("closed-volume.png"); WallA = [System.Drawing.Color]::FromArgb(255, 20, 28, 44); WallB = [System.Drawing.Color]::FromArgb(255, 32, 38, 62) },
    @{ Title = "AI Agent Live Status"; Desc = "Claude Code, Copilot, Cursor & Codex activity bubbles"; Shots = @("closed-agent-alert.png"); WallA = [System.Drawing.Color]::FromArgb(255, 28, 22, 44); WallB = [System.Drawing.Color]::FromArgb(255, 48, 28, 52) },
    @{ Title = "While You Were Away"; Desc = "Smart summary of missed events & alerts on return"; Shots = @("closed-away.png"); WallA = [System.Drawing.Color]::FromArgb(255, 14, 32, 42); WallB = [System.Drawing.Color]::FromArgb(255, 20, 48, 58) }
)

$cw = 485.0; $ch = 218.0; $gx = 24.0; $gy = 20.0
$cTitleF = New-Object System.Drawing.Font("Segoe UI", 13.0, [System.Drawing.FontStyle]::Bold)
$cDescF = New-Object System.Drawing.Font("Segoe UI", 10.5, [System.Drawing.FontStyle]::Regular)
for ($i = 0; $i -lt $compactCards.Count; $i++) {
    $col = $i % 3
    $row = [Math]::Floor($i / 3)
    $cx = 48.0 + $col * ($cw + $gx)
    $cy = 116.0 + $row * ($ch + $gy)
    $item = $compactCards[$i]
    Draw-DesktopCard $g $cx $cy $cw $ch 18.0 10.0 $item.WallA $item.WallB

    if ($item.Shots.Count -eq 2) {
        $null = Draw-ShotAnchored $g $item.Shots[0] ($cx + $cw * 0.28) ($cy + 8.0) 190.0 96.0
        $null = Draw-ShotAnchored $g $item.Shots[1] ($cx + $cw * 0.72) ($cy + 8.0) 190.0 96.0
    } else {
        $null = Draw-ShotAnchored $g $item.Shots[0] ($cx + $cw * 0.5) ($cy + 8.0) ($cw - 36.0) 122.0
    }

    $g.DrawString($item.Title, $cTitleF, $hTitleB, [System.Drawing.RectangleF]::new($cx + 18.0, $cy + $ch - 64.0, $cw - 36.0, 24.0))
    $g.DrawString($item.Desc, $cDescF, $hSubB, [System.Drawing.RectangleF]::new($cx + 18.0, $cy + $ch - 38.0, $cw - 36.0, 24.0))
}
$cDescF.Dispose(); $cTitleF.Dispose(); $hSubB.Dispose(); $hTitleB.Dispose(); $hSubF.Dispose(); $hTitleF.Dispose()
$bmp.Save((Join-Path $outDir "showcase-compact.png"), [System.Drawing.Imaging.ImageFormat]::Png)
$g.Dispose(); $bmp.Dispose()

# ---------------------------------------------------------------------------
# 3. 10 INTERACTIVE TABS SHOWCASE (1600 x 1480)
# ---------------------------------------------------------------------------
$bmp, $g = New-Canvas 1600 1480
Draw-Backdrop $g 1600 1480
Draw-RadialGlow $g 360 360 520 360 ([System.Drawing.Color]::FromArgb(26, 138, 180, 255))
Draw-RadialGlow $g 1240 980 520 360 ([System.Drawing.Color]::FromArgb(26, 167, 139, 250))

$hTitleF = New-Object System.Drawing.Font("Segoe UI", 24.0, [System.Drawing.FontStyle]::Bold)
$hSubF = New-Object System.Drawing.Font("Segoe UI", 12.0, [System.Drawing.FontStyle]::Regular)
$hTitleB = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(255, 255, 255, 255))
$hSubB = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(200, 160, 174, 198))
$g.DrawString("10 Interactive Notch Tabs", $hTitleF, $hTitleB, 48.0, 26.0)
$g.DrawString("Hover the notch or press Ctrl+Alt+Y to expand into a full media, productivity, and utility command center.", $hSubF, $hSubB, 50.0, 70.0)

$tabCards = @(
    @{ Num = "01"; Title = "Media & Synced Lyrics"; Desc = "SMTC playback, album-art accent & LRCLIB karaoke scroll"; Shot = "open-media.png"; WallA = [System.Drawing.Color]::FromArgb(255, 22, 24, 56); WallB = [System.Drawing.Color]::FromArgb(255, 44, 28, 78) },
    @{ Num = "02"; Title = "Calendar & Agenda"; Desc = "Interactive month grid, Google Calendar & one-click Join"; Shot = "open-agenda.png"; WallA = [System.Drawing.Color]::FromArgb(255, 16, 28, 48); WallB = [System.Drawing.Color]::FromArgb(255, 24, 42, 68) },
    @{ Num = "03"; Title = "Focus (Pomodoro)"; Desc = "Focus/break cycles, task linking & 7-day progress bars"; Shot = "open-focus.png"; WallA = [System.Drawing.Color]::FromArgb(255, 20, 26, 44); WallB = [System.Drawing.Color]::FromArgb(255, 34, 38, 64) },
    @{ Num = "04"; Title = "Water Tracker"; Desc = "One-tap hydration logging, undo & hourly reminders"; Shot = "open-water.png"; WallA = [System.Drawing.Color]::FromArgb(255, 12, 30, 48); WallB = [System.Drawing.Color]::FromArgb(255, 18, 48, 72) },
    @{ Num = "05"; Title = "Prayer Times (Celestial Horizon)"; Desc = "Astronomical times, Hijri date, sunset arc & pre-alerts"; Shot = "open-pray.png"; WallA = [System.Drawing.Color]::FromArgb(255, 14, 32, 34); WallB = [System.Drawing.Color]::FromArgb(255, 24, 46, 44) },
    @{ Num = "06"; Title = "Tasks & Natural Reminders"; Desc = "Natural time parsing ('at 3pm') & Google Tasks sync"; Shot = "open-tasks.png"; WallA = [System.Drawing.Color]::FromArgb(255, 22, 24, 40); WallB = [System.Drawing.Color]::FromArgb(255, 34, 36, 58) },
    @{ Num = "07"; Title = "File Shelf & Magnetic Drop"; Desc = "Drag files onto the notch to stash, preview & drag out"; Shot = "open-shelf-drop.png"; WallA = [System.Drawing.Color]::FromArgb(255, 24, 22, 52); WallB = [System.Drawing.Color]::FromArgb(255, 40, 32, 76) },
    @{ Num = "08"; Title = "Notification Center"; Desc = "Windows toast listener, unread badge & Mochi alerts"; Shot = "open-notifications.png"; WallA = [System.Drawing.Color]::FromArgb(255, 28, 22, 42); WallB = [System.Drawing.Color]::FromArgb(255, 46, 28, 54) },
    @{ Num = "09"; Title = "Clipboard History"; Desc = "Text, files & bitmap capture with pinning & live preview"; Shot = "open-clipboard.png"; WallA = [System.Drawing.Color]::FromArgb(255, 18, 28, 46); WallB = [System.Drawing.Color]::FromArgb(255, 28, 42, 68) },
    @{ Num = "10"; Title = "Weather (Atmospheric Horizon)"; Desc = "Open-Meteo forecast, hourly curve & rain warnings"; Shot = "open-weather.png"; WallA = [System.Drawing.Color]::FromArgb(255, 26, 26, 42); WallB = [System.Drawing.Color]::FromArgb(255, 44, 38, 56) }
)

$cw = 740.0; $ch = 252.0; $gx = 24.0; $gy = 18.0
$cTitleF = New-Object System.Drawing.Font("Segoe UI", 13.0, [System.Drawing.FontStyle]::Bold)
$cDescF = New-Object System.Drawing.Font("Segoe UI", 10.5, [System.Drawing.FontStyle]::Regular)
$badgeF = New-Object System.Drawing.Font("Segoe UI", 9.5, [System.Drawing.FontStyle]::Bold)
$badgeFill = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(45, 138, 180, 255))
$badgeTxtB = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(255, 165, 200, 255))
$sfCenter = New-Object System.Drawing.StringFormat
$sfCenter.Alignment = [System.Drawing.StringAlignment]::Center
$sfCenter.LineAlignment = [System.Drawing.StringAlignment]::Center

for ($i = 0; $i -lt $tabCards.Count; $i++) {
    $col = $i % 2
    $row = [Math]::Floor($i / 2)
    $cx = 48.0 + $col * ($cw + $gx)
    $cy = 116.0 + $row * ($ch + $gy)
    $item = $tabCards[$i]
    Draw-DesktopCard $g $cx $cy $cw $ch 16.0 8.0 $item.WallA $item.WallB
    $null = Draw-ShotAnchored $g $item.Shot ($cx + $cw * 0.5) ($cy + 6.0) ($cw - 40.0) 166.0

    $bp = New-RR ($cx + 16.0) ($cy + $ch - 58.0) 30.0 22.0 7.0
    $g.FillPath($badgeFill, $bp)
    $g.DrawString($item.Num, $badgeF, $badgeTxtB, [System.Drawing.RectangleF]::new($cx + 16.0, $cy + $ch - 58.0, 30.0, 22.0), $sfCenter)
    $bp.Dispose()

    $g.DrawString($item.Title, $cTitleF, $hTitleB, [System.Drawing.RectangleF]::new($cx + 54.0, $cy + $ch - 60.0, $cw - 72.0, 24.0))
    $g.DrawString($item.Desc, $cDescF, $hSubB, [System.Drawing.RectangleF]::new($cx + 16.0, $cy + $ch - 32.0, $cw - 32.0, 22.0))
}
$sfCenter.Dispose(); $badgeTxtB.Dispose(); $badgeFill.Dispose(); $badgeF.Dispose()
$cDescF.Dispose(); $cTitleF.Dispose(); $hSubB.Dispose(); $hTitleB.Dispose(); $hSubF.Dispose(); $hTitleF.Dispose()
$bmp.Save((Join-Path $outDir "showcase-tabs.png"), [System.Drawing.Imaging.ImageFormat]::Png)
$g.Dispose(); $bmp.Dispose()

# ---------------------------------------------------------------------------
# 4. AI AGENTS & CLI SHOWCASE (1600 x 580)
# ---------------------------------------------------------------------------
$bmp, $g = New-Canvas 1600 580
Draw-Backdrop $g 1600 580
Draw-RadialGlow $g 460 320 520 260 ([System.Drawing.Color]::FromArgb(34, 110, 158, 255))
Draw-RadialGlow $g 1260 320 420 260 ([System.Drawing.Color]::FromArgb(28, 217, 119, 87))

$hTitleF = New-Object System.Drawing.Font("Segoe UI", 24.0, [System.Drawing.FontStyle]::Bold)
$hSubF = New-Object System.Drawing.Font("Segoe UI", 12.0, [System.Drawing.FontStyle]::Regular)
$hTitleB = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(255, 255, 255, 255))
$hSubB = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(200, 160, 174, 198))
$g.DrawString("AI Coding Agents & CLI Integration", $hTitleF, $hTitleB, 48.0, 26.0)
$g.DrawString("Monitor long builds, test suites, and AI coding sessions directly beside the notch via local named-pipe IPC.", $hSubF, $hSubB, 50.0, 70.0)

# Left Desktop Panel
$lx = 48.0; $ly = 116.0; $lw = 920.0; $lh = 420.0
$lp = New-RR $lx $ly $lw $lh 20.0
$state = $g.Save()
$g.SetClip($lp)
$lRect = [System.Drawing.RectangleF]::new($lx, $ly, $lw, $lh)
$lGrad = New-Object System.Drawing.Drawing2D.LinearGradientBrush(
    $lRect,
    [System.Drawing.Color]::FromArgb(255, 14, 22, 44),
    [System.Drawing.Color]::FromArgb(255, 26, 22, 54),
    35.0
)
$g.FillRectangle($lGrad, $lRect)
$lGrad.Dispose()
$bezelB = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(255, 0, 0, 0))
$g.FillRectangle($bezelB, $lx, $ly, $lw, 12.0)
$bezelB.Dispose()

$null = Draw-ShotAnchored $g "open-copilot.png" ($lx + $lw * 0.5) ($ly + 10.0) 880.0 210.0
$g.Restore($state)
$lPen = New-Object System.Drawing.Pen([System.Drawing.Color]::FromArgb(42, 255, 255, 255), 1.4)
$g.DrawPath($lPen, $lp)
$lPen.Dispose(); $lp.Dispose()

# Two mini top-bezel sub-cards inside bottom of left panel
$subW = 428.0; $subH = 154.0; $subY = $ly + 244.0
$subItems = @(
    @{ X = $lx + 22.0; Shot = "closed-copilot.png"; Label = "Live Agent Progress Bubble"; WallA = [System.Drawing.Color]::FromArgb(255, 18, 28, 54); WallB = [System.Drawing.Color]::FromArgb(255, 28, 36, 72) },
    @{ X = $lx + 470.0; Shot = "closed-agent-alert.png"; Label = "Completion & Attention Alert"; WallA = [System.Drawing.Color]::FromArgb(255, 32, 22, 46); WallB = [System.Drawing.Color]::FromArgb(255, 52, 28, 54) }
)
$subLblF = New-Object System.Drawing.Font("Segoe UI", 10.5, [System.Drawing.FontStyle]::Bold)
foreach ($si in $subItems) {
    $sp = New-RR $si.X $subY $subW $subH 14.0
    $sState = $g.Save()
    $g.SetClip($sp)
    $sRect = [System.Drawing.RectangleF]::new($si.X, $subY, $subW, $subH)
    $sGrad = New-Object System.Drawing.Drawing2D.LinearGradientBrush($sRect, $si.WallA, $si.WallB, 30.0)
    $g.FillRectangle($sGrad, $sRect)
    $sGrad.Dispose()
    $sb = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(255, 0, 0, 0))
    $g.FillRectangle($sb, $si.X, $subY, $subW, 9.0)
    $sb.Dispose()
    $null = Draw-ShotAnchored $g $si.Shot ($si.X + $subW * 0.5) ($subY + 7.0) ($subW - 24.0) 86.0
    $sfC = New-Object System.Drawing.StringFormat
    $sfC.Alignment = [System.Drawing.StringAlignment]::Center
    $g.DrawString($si.Label, $subLblF, $hSubB, [System.Drawing.RectangleF]::new($si.X, $subY + $subH - 34.0, $subW, 24.0), $sfC)
    $sfC.Dispose()
    $g.Restore($sState)
    $sPen = New-Object System.Drawing.Pen([System.Drawing.Color]::FromArgb(38, 255, 255, 255), 1.2)
    $g.DrawPath($sPen, $sp)
    $sPen.Dispose(); $sp.Dispose()
}
$subLblF.Dispose()

# Right Panel: 4 Agent Cards + CLI Snippet
$rx = 992.0; $ry = 116.0; $rw = 560.0
$agents = @(
    @("GitHub Copilot", "VS Code hooks + Ask Copilot prompt bar", [System.Drawing.Color]::FromArgb(255, 110, 158, 255)),
    @("Claude Code", "Native hooks, WSL distros & plugin support", [System.Drawing.Color]::FromArgb(255, 217, 119, 87)),
    @("Cursor", "Prompt, tool, shell & file-edit tracking", [System.Drawing.Color]::FromArgb(255, 226, 232, 240)),
    @("Codex", "Session, permission & interrupt hooks", [System.Drawing.Color]::FromArgb(255, 52, 211, 153))
)
$aTitleF = New-Object System.Drawing.Font("Segoe UI", 12.5, [System.Drawing.FontStyle]::Bold)
$aDescF = New-Object System.Drawing.Font("Segoe UI", 10.0, [System.Drawing.FontStyle]::Regular)
$aw = 272.0; $ah = 112.0
for ($i = 0; $i -lt 4; $i++) {
    $col = $i % 2
    $row = [Math]::Floor($i / 2)
    $ax = $rx + $col * ($aw + 16.0)
    $ay = $ry + $row * ($ah + 14.0)
    $ap = New-RR $ax $ay $aw $ah 14.0
    $af = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(255, 14, 18, 30))
    $apen = New-Object System.Drawing.Pen([System.Drawing.Color]::FromArgb(36, 255, 255, 255), 1.2)
    $g.FillPath($af, $ap)
    $g.DrawPath($apen, $ap)
    $dotB = New-Object System.Drawing.SolidBrush($agents[$i][2])
    $g.FillEllipse($dotB, $ax + 16.0, $ay + 20.0, 10.0, 10.0)
    $g.DrawString($agents[$i][0], $aTitleF, $hTitleB, [System.Drawing.RectangleF]::new($ax + 34.0, $ay + 13.0, $aw - 48.0, 24.0))
    $g.DrawString($agents[$i][1], $aDescF, $hSubB, [System.Drawing.RectangleF]::new($ax + 16.0, $ay + 44.0, $aw - 32.0, 54.0))
    $dotB.Dispose(); $apen.Dispose(); $af.Dispose(); $ap.Dispose()
}
$aDescF.Dispose(); $aTitleF.Dispose()

# CLI Terminal Card
$ty = $ry + 254.0; $th = 166.0
$tp = New-RR $rx $ty $rw $th 14.0
$tf = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(255, 9, 12, 20))
$tpen = New-Object System.Drawing.Pen([System.Drawing.Color]::FromArgb(48, 138, 180, 255), 1.2)
$g.FillPath($tf, $tp)
$g.DrawPath($tpen, $tp)
$termHdrF = New-Object System.Drawing.Font("Segoe UI", 10.5, [System.Drawing.FontStyle]::Bold)
$termCodeF = New-Object System.Drawing.Font("Consolas", 10.5, [System.Drawing.FontStyle]::Regular)
$termHdrB = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(255, 138, 180, 255))
$termCodeB = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(240, 226, 232, 240))
$g.DrawString("mochi-cli $dot Named Pipe IPC (\\.\pipe\Mochi.{SID}.{SessionID})", $termHdrF, $termHdrB, $rx + 16.0, $ty + 14.0)
$cliText = "mochi-cli hook setup copilot`nmochi-cli skill install`nmochi-cli activity set build --title `"Release`" --progress 72%`nmochi-cli activity done build --title `"Release`" --text `"ready`""
$g.DrawString($cliText, $termCodeF, $termCodeB, [System.Drawing.RectangleF]::new($rx + 16.0, $ty + 44.0, $rw - 32.0, 110.0))
$termCodeB.Dispose(); $termHdrB.Dispose(); $termCodeF.Dispose(); $termHdrF.Dispose()
$tpen.Dispose(); $tf.Dispose(); $tp.Dispose()

$hSubB.Dispose(); $hTitleB.Dispose(); $hSubF.Dispose(); $hTitleF.Dispose()
$bmp.Save((Join-Path $outDir "showcase-agents.png"), [System.Drawing.Imaging.ImageFormat]::Png)
$g.Dispose(); $bmp.Dispose()
}

# ---------------------------------------------------------------------------
# 5. PERFORMANCE & BENCHMARK SHOWCASE (1600 x 520)
# ---------------------------------------------------------------------------
$bmp, $g = New-Canvas 1600 520
Draw-Backdrop $g 1600 520
Draw-RadialGlow $g 420 280 480 240 ([System.Drawing.Color]::FromArgb(30, 56, 189, 248))
Draw-RadialGlow $g 1220 280 480 240 ([System.Drawing.Color]::FromArgb(28, 52, 211, 153))

$hTitleF = New-Object System.Drawing.Font("Segoe UI", 24.0, [System.Drawing.FontStyle]::Bold)
$hSubF = New-Object System.Drawing.Font("Segoe UI", 12.0, [System.Drawing.FontStyle]::Regular)
$hTitleB = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(255, 255, 255, 255))
$hSubB = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(200, 160, 174, 198))
$g.DrawString("Lightweight Native Performance", $hTitleF, $hTitleB, 48.0, 26.0)
$g.DrawString("Measured live on Windows (8 logical cores) $emdash zero Electron, zero WebView2, 2.37 MB release binary, 49 loaded DLLs.", $hSubF, $hSubB, 50.0, 70.0)

# Left Card: Windows Task Manager Style Card
$lx = 48.0; $ly = 116.0; $lw = 820.0; $lh = 360.0
$lp = New-RR $lx $ly $lw $lh 18.0
$lf = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(255, 17, 20, 30))
$lpen = New-Object System.Drawing.Pen([System.Drawing.Color]::FromArgb(42, 255, 255, 255), 1.3)
$g.FillPath($lf, $lp)
$g.DrawPath($lpen, $lp)

$tmHdrF = New-Object System.Drawing.Font("Segoe UI", 13.5, [System.Drawing.FontStyle]::Bold)
$tmColF = New-Object System.Drawing.Font("Segoe UI", 11.5, [System.Drawing.FontStyle]::Regular)
$tmValF = New-Object System.Drawing.Font("Segoe UI", 12.5, [System.Drawing.FontStyle]::Bold)
$tmCapF = New-Object System.Drawing.Font("Segoe UI", 10.5, [System.Drawing.FontStyle]::Regular)
$tmMutedB = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(185, 176, 188, 210))
$tmDivPen = New-Object System.Drawing.Pen([System.Drawing.Color]::FromArgb(28, 255, 255, 255), 1.0)

$g.DrawString("Processes", $tmHdrF, $hTitleB, $lx + 22.0, $ly + 18.0)
$g.DrawLine($tmDivPen, $lx + 1.0, $ly + 54.0, $lx + $lw - 1.0, $ly + 54.0)

$sfRight = New-Object System.Drawing.StringFormat
$sfRight.Alignment = [System.Drawing.StringAlignment]::Far
$sfRight.LineAlignment = [System.Drawing.StringAlignment]::Center
$sfLeftC = New-Object System.Drawing.StringFormat
$sfLeftC.Alignment = [System.Drawing.StringAlignment]::Near
$sfLeftC.LineAlignment = [System.Drawing.StringAlignment]::Center

$hdrY = $ly + 64.0; $hdrH = 34.0
$g.DrawString("Name", $tmColF, $tmMutedB, [System.Drawing.RectangleF]::new($lx + 22.0, $hdrY, 220.0, $hdrH), $sfLeftC)
$g.DrawString("CPU", $tmColF, $tmMutedB, [System.Drawing.RectangleF]::new($lx + 280.0, $hdrY, 110.0, $hdrH), $sfRight)
$g.DrawString("Memory", $tmColF, $tmMutedB, [System.Drawing.RectangleF]::new($lx + 400.0, $hdrY, 130.0, $hdrH), $sfRight)
$g.DrawString("Disk", $tmColF, $tmMutedB, [System.Drawing.RectangleF]::new($lx + 540.0, $hdrY, 120.0, $hdrH), $sfRight)
$g.DrawString("Network", $tmColF, $tmMutedB, [System.Drawing.RectangleF]::new($lx + 670.0, $hdrY, 126.0, $hdrH), $sfRight)

# Highlighted Mochi Process Row with Task Manager heat cells
$rowY = $ly + 106.0; $rowH = 58.0
$rowBg = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(255, 24, 30, 46))
$g.FillRectangle($rowBg, $lx + 2.0, $rowY, $lw - 4.0, $rowH)
$rowBg.Dispose()

$cpuCell = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(255, 34, 46, 74))
$memCell = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(255, 44, 62, 102))
$idleCell = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(255, 28, 36, 56))
$g.FillRectangle($cpuCell, $lx + 286.0, $rowY, 114.0, $rowH)
$g.FillRectangle($memCell, $lx + 402.0, $rowY, 138.0, $rowH)
$g.FillRectangle($idleCell, $lx + 542.0, $rowY, 128.0, $rowH)
$g.FillRectangle($idleCell, $lx + 672.0, $rowY, 146.0, $rowH)
$cpuCell.Dispose(); $memCell.Dispose(); $idleCell.Dispose()

# Draw crisp vector Mochi app icon (obsidian rounded square + gradient rim + Cozmo eyes)
$ix = $lx + 24.0; $iy = $rowY + 13.0; $is = 32.0
$rimRect = [System.Drawing.RectangleF]::new($ix, $iy, $is, $is)
$rimGrad = New-Object System.Drawing.Drawing2D.LinearGradientBrush(
    $rimRect,
    [System.Drawing.Color]::FromArgb(255, 138, 180, 255),
    [System.Drawing.Color]::FromArgb(255, 124, 58, 237),
    45.0
)
$rimPath = New-RR $ix $iy $is $is 9.0
$g.FillPath($rimGrad, $rimPath)
$innerPath = New-RR ($ix + 2.0) ($iy + 2.0) ($is - 4.0) ($is - 4.0) 7.5
$innerB = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(255, 10, 12, 18))
$g.FillPath($innerB, $innerPath)
$eyeB = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(255, 248, 249, 252))
foreach ($sign in @(-1, 1)) {
    $ep = New-RR ($ix + $is * 0.5 + $sign * 5.2 - 2.8) ($iy + $is * 0.5 - 3.6) 5.6 7.2 2.0
    $g.FillPath($eyeB, $ep)
    $ep.Dispose()
}
$eyeB.Dispose(); $innerB.Dispose(); $innerPath.Dispose(); $rimPath.Dispose(); $rimGrad.Dispose()
$g.DrawString("Mochi", $tmValF, $hTitleB, [System.Drawing.RectangleF]::new($lx + 68.0, $rowY, 200.0, $rowH), $sfLeftC)
$g.DrawString("0.3%", $tmValF, $hTitleB, [System.Drawing.RectangleF]::new($lx + 280.0, $rowY, 110.0, $rowH), $sfRight)
$g.DrawString("8.7 MB", $tmValF, $hTitleB, [System.Drawing.RectangleF]::new($lx + 400.0, $rowY, 130.0, $rowH), $sfRight)
$g.DrawString("0 MB/s", $tmValF, $hTitleB, [System.Drawing.RectangleF]::new($lx + 540.0, $rowY, 120.0, $rowH), $sfRight)
$g.DrawString("0 Mbps", $tmValF, $hTitleB, [System.Drawing.RectangleF]::new($lx + 670.0, $rowY, 126.0, $rowH), $sfRight)

# 3 Mini Telemetry Cards inside Left Card
$miniStats = @(
    @("2.37 MB", "Mochi.exe (LTO + Strip)", [System.Drawing.Color]::FromArgb(255, 138, 180, 255)),
    @("0.3 MB Idle", "Trimmed Working Set", [System.Drawing.Color]::FromArgb(255, 56, 189, 248)),
    @("49 DLLs", "232 Handles (No GPU Bloat)", [System.Drawing.Color]::FromArgb(255, 52, 211, 153))
)
$mw = 244.0; $mh = 86.0; $my = $ly + 194.0
$mValF = New-Object System.Drawing.Font("Segoe UI", 16.0, [System.Drawing.FontStyle]::Bold)
$mLblF = New-Object System.Drawing.Font("Segoe UI", 10.0, [System.Drawing.FontStyle]::Regular)
for ($i = 0; $i -lt 3; $i++) {
    $mx = $lx + 22.0 + $i * ($mw + 22.0)
    $mp = New-RR $mx $my $mw $mh 12.0
    $mf = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(255, 22, 27, 42))
    $mpen = New-Object System.Drawing.Pen([System.Drawing.Color]::FromArgb(32, 255, 255, 255), 1.0)
    $g.FillPath($mf, $mp)
    $g.DrawPath($mpen, $mp)
    $mb = New-Object System.Drawing.SolidBrush($miniStats[$i][2])
    $g.DrawString($miniStats[$i][0], $mValF, $mb, $mx + 16.0, $my + 14.0)
    $g.DrawString($miniStats[$i][1], $mLblF, $tmMutedB, $mx + 16.0, $my + 50.0)
    $mb.Dispose(); $mpen.Dispose(); $mf.Dispose(); $mp.Dispose()
}
$mLblF.Dispose(); $mValF.Dispose()

$g.DrawString("Windows Task Manager during active 100 Hz transitions (8.68 MB PrivateWS; 0.29 MB when idle-trimmed).", $tmCapF, $tmMutedB, $lx + 22.0, $ly + 312.0)

$sfLeftC.Dispose(); $sfRight.Dispose(); $tmDivPen.Dispose(); $tmMutedB.Dispose()
$tmCapF.Dispose(); $tmValF.Dispose(); $tmColF.Dispose(); $tmHdrF.Dispose()
$lpen.Dispose(); $lf.Dispose(); $lp.Dispose()

# Right Card: Direct2D Frame Timing Benchmark
$rx = 892.0; $ry = 116.0; $rw = 660.0; $rh = 360.0
$rp = New-RR $rx $ry $rw $rh 18.0
$rf = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(255, 14, 18, 30))
$rpen = New-Object System.Drawing.Pen([System.Drawing.Color]::FromArgb(42, 255, 255, 255), 1.3)
$g.FillPath($rf, $rp)
$g.DrawPath($rpen, $rp)

$bHdrF = New-Object System.Drawing.Font("Segoe UI", 13.0, [System.Drawing.FontStyle]::Bold)
$bKpiF = New-Object System.Drawing.Font("Segoe UI", 20.0, [System.Drawing.FontStyle]::Bold)
$bLblF = New-Object System.Drawing.Font("Segoe UI", 10.0, [System.Drawing.FontStyle]::Regular)
$bBarF = New-Object System.Drawing.Font("Segoe UI", 10.5, [System.Drawing.FontStyle]::Bold)
$bMutedB = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(185, 176, 188, 210))
$g.DrawString("Direct2D Frame Budget (3,527 frames $dot 12 cycles)", $bHdrF, $hTitleB, $rx + 22.0, $ry + 18.0)

$kpis = @(
    @("4.13 ms", "Avg Render Work", [System.Drawing.Color]::FromArgb(255, 52, 211, 153)),
    @("6.18 ms", "p95 Render Work", [System.Drawing.Color]::FromArgb(255, 138, 180, 255)),
    @("10.21 ms", "Avg Frame Interval", [System.Drawing.Color]::FromArgb(255, 56, 189, 248))
)
for ($i = 0; $i -lt 3; $i++) {
    $kx = $rx + 22.0 + $i * 208.0
    $kb = New-Object System.Drawing.SolidBrush($kpis[$i][2])
    $g.DrawString($kpis[$i][0], $bKpiF, $kb, $kx, $ry + 54.0)
    $g.DrawString($kpis[$i][1], $bLblF, $bMutedB, $kx, $ry + 92.0)
    $kb.Dispose()
}

# Horizontal Bar Chart vs 10.0 ms (100 Hz) budget
$bars = @(
    @("HUD Exit", 3.12, [System.Drawing.Color]::FromArgb(255, 52, 211, 153)),
    @("HUD Enter", 3.60, [System.Drawing.Color]::FromArgb(255, 52, 211, 153)),
    @("Collapse", 3.60, [System.Drawing.Color]::FromArgb(255, 56, 189, 248)),
    @("Tasks Tab", 4.39, [System.Drawing.Color]::FromArgb(255, 138, 180, 255)),
    @("Agenda Tab", 4.86, [System.Drawing.Color]::FromArgb(255, 138, 180, 255)),
    @("Media + Lyrics", 4.89, [System.Drawing.Color]::FromArgb(255, 138, 180, 255)),
    @("Expand", 4.91, [System.Drawing.Color]::FromArgb(255, 167, 139, 250))
)
$barX = $rx + 148.0; $barMaxW = 410.0; $maxMs = 11.0
$trackB = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(22, 255, 255, 255))
$sfR = New-Object System.Drawing.StringFormat
$sfR.Alignment = [System.Drawing.StringAlignment]::Far
$sfR.LineAlignment = [System.Drawing.StringAlignment]::Center
$sfL = New-Object System.Drawing.StringFormat
$sfL.Alignment = [System.Drawing.StringAlignment]::Near
$sfL.LineAlignment = [System.Drawing.StringAlignment]::Center

# 10.0 ms budget dashed line
$budgetX = $barX + [float]((10.0 / $maxMs) * $barMaxW)
$dashPen = New-Object System.Drawing.Pen([System.Drawing.Color]::FromArgb(110, 251, 191, 36), 1.4)
$dashPen.DashStyle = [System.Drawing.Drawing2D.DashStyle]::Dash
$g.DrawLine($dashPen, $budgetX, $ry + 126.0, $budgetX, $ry + 322.0)
$budgetB = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(220, 251, 191, 36))
$g.DrawString("10 ms (100 Hz budget)", $bLblF, $budgetB, $budgetX - 136.0, $ry + 326.0)
$budgetB.Dispose(); $dashPen.Dispose()

for ($i = 0; $i -lt $bars.Count; $i++) {
    $by = $ry + 130.0 + $i * 27.0
    $g.DrawString($bars[$i][0], $bLblF, $bMutedB, [System.Drawing.RectangleF]::new($rx + 22.0, $by, 118.0, 20.0), $sfL)
    $tp = New-RR $barX ($by + 3.0) $barMaxW 14.0 7.0
    $g.FillPath($trackB, $tp)
    $tp.Dispose()
    $bw = [float](($bars[$i][1] / $maxMs) * $barMaxW)
    $bp = New-RR $barX ($by + 3.0) $bw 14.0 7.0
    $bb = New-Object System.Drawing.SolidBrush($bars[$i][2])
    $g.FillPath($bb, $bp)
    $bb.Dispose(); $bp.Dispose()
    $valTxt = "{0:N2} ms" -f $bars[$i][1]
    $g.DrawString($valTxt, $bBarF, $hTitleB, [System.Drawing.RectangleF]::new($barX + $barMaxW + 8.0, $by, 76.0, 20.0), $sfR)
}
$sfL.Dispose(); $sfR.Dispose(); $trackB.Dispose()
$bMutedB.Dispose(); $bBarF.Dispose(); $bLblF.Dispose(); $bKpiF.Dispose(); $bHdrF.Dispose()
$rpen.Dispose(); $rf.Dispose(); $rp.Dispose()

$hSubB.Dispose(); $hTitleB.Dispose(); $hSubF.Dispose(); $hTitleF.Dispose()
$bmp.Save((Join-Path $outDir "showcase-performance.png"), [System.Drawing.Imaging.ImageFormat]::Png)
$g.Dispose(); $bmp.Dispose()

"Generated README visual assets in $outDir"
