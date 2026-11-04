<#
.SYNOPSIS
    Draws the Inno Setup wizard artwork (assets\wizard.bmp and wizard-small.bmp).

.DESCRIPTION
    Uses the classic System.Drawing stack, so run it with Windows PowerShell:

        powershell.exe -NoProfile -ExecutionPolicy Bypass -File make-art.ps1

    build.ps1 calls it automatically.
#>

[CmdletBinding()]
param(
    [string]$BigPath,
    [string]$SmallPath,
    [switch]$Force
)

$ErrorActionPreference = 'Stop'
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
if (-not $BigPath) { $BigPath = Join-Path $here 'assets\wizard.bmp' }
if (-not $SmallPath) { $SmallPath = Join-Path $here 'assets\wizard-small.png' }
if ((Test-Path $BigPath) -and (Test-Path $SmallPath) -and -not $Force) {
    Write-Host 'Wizard artwork already exists.'
    exit 0
}

Add-Type -ReferencedAssemblies 'System.Drawing' -TypeDefinition @'
using System;
using System.Drawing;
using System.Drawing.Drawing2D;
using System.Drawing.Imaging;

public static class DhakaFlixArt
{
    public static void Build(string bigPath, string smallPath)
    {
        DrawBig(bigPath);
        DrawSmall(smallPath);
    }

    private static void DrawBig(string path)
    {
        using (var bmp = new Bitmap(164, 314, PixelFormat.Format24bppRgb))
        {
            using (Graphics g = Graphics.FromImage(bmp))
            {
                g.SmoothingMode = SmoothingMode.AntiAlias;
                g.TextRenderingHint = System.Drawing.Text.TextRenderingHint.ClearTypeGridFit;

                var canvas = new Rectangle(0, 0, 164, 314);
                using (var brush = new LinearGradientBrush(canvas, Color.FromArgb(10, 10, 26), Color.FromArgb(27, 27, 52), 90f))
                {
                    g.FillRectangle(brush, canvas);
                }

                using (var glow = new SolidBrush(Color.FromArgb(26, 67, 56, 202)))
                {
                    g.FillEllipse(glow, -70, 200, 260, 260);
                }

                using (var glow = new SolidBrush(Color.FromArgb(18, 34, 197, 94)))
                {
                    g.FillEllipse(glow, 60, 250, 180, 180);
                }

                DrawMark(g, new RectangleF(24, 28, 46, 46), 12f, true);

                using (var font = new Font("Segoe UI", 15f, FontStyle.Bold, GraphicsUnit.Point))
                using (var brush = new SolidBrush(Color.FromArgb(248, 250, 252)))
                {
                    g.DrawString("DhakaFlix", font, brush, 23f, 88f);
                }

                using (var font = new Font("Segoe UI", 8.5f, FontStyle.Regular, GraphicsUnit.Point))
                using (var brush = new SolidBrush(Color.FromArgb(165, 176, 194)))
                {
                    g.DrawString("Stremio addon\nrelay for BDIX\nmirrors", font, brush, new RectangleF(24, 120, 122, 66));
                }

                using (var pen = new Pen(Color.FromArgb(60, 99, 102, 241), 1f))
                {
                    g.DrawLine(pen, 24, 236, 140, 236);
                }

                using (var font = new Font("Segoe UI", 7f, FontStyle.Bold, GraphicsUnit.Point))
                using (var brush = new SolidBrush(Color.FromArgb(130, 140, 158)))
                {
                    g.DrawString("RELAY  /  STREMIO  /  BDIX", font, brush, 23f, 246f);
                }

                using (var font = new Font("Segoe UI", 7f, FontStyle.Regular, GraphicsUnit.Point))
                using (var brush = new SolidBrush(Color.FromArgb(104, 114, 132)))
                {
                    g.DrawString("v1.1.0", font, brush, 23f, 284f);
                }
            }

            bmp.Save(path, ImageFormat.Bmp);
        }
    }

    private static void DrawSmall(string path)
    {
        using (var bmp = new Bitmap(55, 55, PixelFormat.Format32bppArgb))
        {
            using (Graphics g = Graphics.FromImage(bmp))
            {
                g.SmoothingMode = SmoothingMode.AntiAlias;
                g.Clear(Color.Transparent);
                DrawMark(g, new RectangleF(4, 4, 47, 47), 13f, true);
            }

            bmp.Save(path, ImageFormat.Png);
        }
    }

    private static void DrawMark(Graphics g, RectangleF tile, float radius, bool withDot)
    {
        using (GraphicsPath path = Round(tile, radius))
        using (var brush = new LinearGradientBrush(tile, Color.FromArgb(67, 56, 202), Color.FromArgb(124, 58, 237), 45f))
        {
            g.FillPath(brush, path);
        }

        using (GraphicsPath path = Round(tile, radius))
        using (var pen = new Pen(Color.FromArgb(70, 199, 210, 254), 1f))
        {
            g.DrawPath(pen, path);
        }

        float w = tile.Width * 0.28f;
        float h = tile.Height * 0.34f;
        float cx = tile.X + tile.Width / 2f + tile.Width * 0.03f;
        float cy = tile.Y + tile.Height / 2f;
        using (var brush = new SolidBrush(Color.White))
        {
            g.FillPolygon(brush, new[]
            {
                new PointF(cx - w * 0.45f, cy - h / 2f),
                new PointF(cx - w * 0.45f, cy + h / 2f),
                new PointF(cx + w * 0.62f, cy),
            });
        }

        if (!withDot) return;

        float dot = tile.Width * 0.20f;
        float dx = tile.Right - dot - tile.Width * 0.12f;
        float dy = tile.Bottom - dot - tile.Height * 0.12f;
        using (var brush = new SolidBrush(Color.FromArgb(67, 56, 202)))
        {
            g.FillEllipse(brush, dx - 3f, dy - 3f, dot + 6f, dot + 6f);
        }
        using (var brush = new SolidBrush(Color.FromArgb(34, 197, 94)))
        {
            g.FillEllipse(brush, dx, dy, dot, dot);
        }
    }

    private static GraphicsPath Round(RectangleF bounds, float radius)
    {
        var path = new GraphicsPath();
        float diameter = radius * 2f;
        var arc = new RectangleF(bounds.X, bounds.Y, diameter, diameter);
        path.AddArc(arc, 180, 90);
        arc.X = bounds.Right - diameter;
        path.AddArc(arc, 270, 90);
        arc.Y = bounds.Bottom - diameter;
        path.AddArc(arc, 0, 90);
        arc.X = bounds.X;
        path.AddArc(arc, 90, 90);
        path.CloseFigure();
        return path;
    }
}
'@

New-Item -ItemType Directory -Force -Path (Split-Path -Parent $BigPath) | Out-Null
[DhakaFlixArt]::Build($BigPath, $SmallPath)
Write-Host "Wizard artwork written: $BigPath, $SmallPath"
