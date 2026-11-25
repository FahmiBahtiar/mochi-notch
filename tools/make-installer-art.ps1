# Generates DPI-matched Inno Setup 6.6+ wizard artwork and plugin icon for Mochi:
#   installer/wizard-image-{100,125,150,175,200}.png
#   installer/wizard-small-{100,125,150,175,200}.png
#   integrations/claude-code/.claude-plugin/icon.png
$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.Drawing

$root = Split-Path $PSScriptRoot -Parent
$outDir = Join-Path $root "installer"
New-Item -ItemType Directory -Force $outDir | Out-Null

function New-RR([float]$x, [float]$y, [float]$w, [float]$h, [float]$r) {
    $p = New-Object System.Drawing.Drawing2D.GraphicsPath
    $r = [Math]::Min($r, [Math]::Min($w, $h) * 0.5)
    $d = $r * 2
    $p.AddArc($x, $y, $d, $d, 180, 90)
    $p.AddArc($x + $w - $d, $y, $d, $d, 270, 90)
    $p.AddArc($x + $w - $d, $y + $h - $d, $d, $d, 0, 90)
    $p.AddArc($x, $y + $h - $d, $d, $d, 90, 90)
    $p.CloseFigure()
    return $p
}

function Save-WizardImage([int]$w, [int]$h, [string]$path) {
    $bmp = New-Object System.Drawing.Bitmap($w, $h, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::HighQuality
    $g.TextRenderingHint = [System.Drawing.Text.TextRenderingHint]::AntiAliasGridFit
    $g.Clear([System.Drawing.Color]::FromArgb(255, 0, 0, 0))

    $s = $w / 202.0
    $cx = $w * 0.5
    $cy = $h * 0.44

    # Subtle dark card framing the Mochi notch silhouette
    $cardW = 146.0 * $s
    $cardH = 76.0 * $s
    $cardPath = New-RR ($cx - $cardW * 0.5) ($cy - $cardH * 0.5) $cardW $cardH (20.0 * $s)
    $cardBrush = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(255, 14, 14, 17))
    $cardPen = New-Object System.Drawing.Pen([System.Drawing.Color]::FromArgb(42, 255, 255, 255), [Math]::Max(1.0, 1.2 * $s))
    $g.FillPath($cardBrush, $cardPath)
    $g.DrawPath($cardPen, $cardPath)
    $cardBrush.Dispose(); $cardPen.Dispose(); $cardPath.Dispose()

    # Cozmo Expressive squircle eyes centered inside
    $gap = 20.0 * $s
    $ew = 22.0 * $s
    $eh = 25.0 * $s
    $er = $ew * 0.36
    $wb = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(255, 248, 249, 252))
    foreach ($sign in @(-1, 1)) {
        $ep = New-RR ($cx + $sign * $gap - $ew * 0.5) ($cy - $eh * 0.5) $ew $eh $er
        $g.FillPath($wb, $ep)
        $ep.Dispose()
    }

    # Wordmark & subtitle
    $titleFont = New-Object System.Drawing.Font("Segoe UI", [float](18.0 * $s), [System.Drawing.FontStyle]::Bold)
    $subFont = New-Object System.Drawing.Font("Segoe UI", [float](9.5 * $s), [System.Drawing.FontStyle]::Regular)
    $mutedB = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(155, 255, 255, 255))
    $sf = New-Object System.Drawing.StringFormat
    $sf.Alignment = [System.Drawing.StringAlignment]::Center
    $g.DrawString("Mochi", $titleFont, $wb, [System.Drawing.RectangleF]::new(0, $cy + 54.0 * $s, $w, 34.0 * $s), $sf)
    $g.DrawString("Windows Notch", $subFont, $mutedB, [System.Drawing.RectangleF]::new(0, $cy + 86.0 * $s, $w, 22.0 * $s), $sf)

    $sf.Dispose(); $mutedB.Dispose(); $subFont.Dispose(); $titleFont.Dispose(); $wb.Dispose(); $g.Dispose()
    $bmp.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
    $bmp.Dispose()
}

function Save-WizardSmall([int]$side, [string]$path) {
    $bmp = New-Object System.Drawing.Bitmap($side, $side, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::HighQuality
    $g.Clear([System.Drawing.Color]::FromArgb(255, 0, 0, 0))

    $s = $side / 58.0
    $cx = $side * 0.5
    $cy = $side * 0.5
    $gap = 11.5 * $s
    $ew = [Math]::Max(2.0, 13.0 * $s)
    $eh = [Math]::Max(2.4, 15.0 * $s)
    $er = $ew * 0.36
    $wb = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(255, 248, 249, 252))
    foreach ($sign in @(-1, 1)) {
        $ep = New-RR ($cx + $sign * $gap - $ew * 0.5) ($cy - $eh * 0.5) $ew $eh $er
        $g.FillPath($wb, $ep)
        $ep.Dispose()
    }
    $wb.Dispose(); $g.Dispose()
    $bmp.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
    $bmp.Dispose()
}

$areas = [ordered]@{
    "100" = @(202, 386, 58)
    "125" = @(269, 515, 77)
    "150" = @(336, 643, 97)
    "175" = @(403, 772, 116)
    "200" = @(430, 824, 124)
}

foreach ($scale in $areas.Keys) {
    $w, $h, $side = $areas[$scale]
    Save-WizardImage $w $h (Join-Path $outDir "wizard-image-$scale.png")
    Save-WizardSmall $side (Join-Path $outDir "wizard-small-$scale.png")
}

function Save-PluginIcon([int]$side, [string]$path) {
    $bmp = New-Object System.Drawing.Bitmap($side, $side, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::HighQuality
    $g.Clear([System.Drawing.Color]::Transparent)

    $pad = $side * 0.04
    $box = $side - 2.0 * $pad
    $rad = $box * 0.24
    $bgPath = New-RR $pad $pad $box $box $rad
    $bgBrush = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(255, 14, 14, 17))
    $rimPen = New-Object System.Drawing.Pen([System.Drawing.Color]::FromArgb(255, 46, 48, 56), [float]($side * 0.02))
    $g.FillPath($bgBrush, $bgPath)
    $g.DrawPath($rimPen, $bgPath)

    $cx = $side * 0.5
    $cy = $side * 0.5
    $gap = $side * 0.185
    $ew = $side * 0.21
    $eh = $side * 0.24
    $er = $ew * 0.36
    $wb = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(255, 248, 249, 252))
    foreach ($sign in @(-1, 1)) {
        $ep = New-RR ($cx + $sign * $gap - $ew * 0.5) ($cy - $eh * 0.5) $ew $eh $er
        $g.FillPath($wb, $ep)
        $ep.Dispose()
    }

    $wb.Dispose(); $rimPen.Dispose(); $bgBrush.Dispose(); $bgPath.Dispose(); $g.Dispose()
    $bmp.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
    $bmp.Dispose()
}

$pluginIcon = Join-Path $root "integrations\claude-code\.claude-plugin\icon.png"
Save-PluginIcon 600 $pluginIcon
"Generated installer wizard artwork in $outDir and plugin icon at $pluginIcon"
