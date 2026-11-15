<#
.SYNOPSIS
    Draws the DhakaFlix Relay icon set used by the Rust build.

.DESCRIPTION
    Produces desktop-rust\assets\app.ico (multi-size Windows icon),
    tray.png (32 px notification-area icon) and icon.png (256 px, used in
    documentation). Flat accent tile with a play glyph; no gradients.

    Run with Windows PowerShell 5.1:
        powershell.exe -NoProfile -ExecutionPolicy Bypass -File make-assets.ps1
#>

[CmdletBinding()]
param(
    [string]$OutputDirectory,
    [switch]$Force
)

$ErrorActionPreference = 'Stop'
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
if (-not $OutputDirectory) { $OutputDirectory = Join-Path $here 'assets' }
New-Item -ItemType Directory -Force -Path $OutputDirectory | Out-Null

if (-not $Force -and (Test-Path (Join-Path $OutputDirectory 'app.ico')) -and (Test-Path (Join-Path $OutputDirectory 'tray.png'))) {
    Write-Host "Assets already exist in $OutputDirectory"
    exit 0
}

Add-Type -ReferencedAssemblies 'System.Drawing' -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Drawing;
using System.Drawing.Drawing2D;
using System.Drawing.Imaging;
using System.IO;

public static class DhakaFlixAssets
{
    static readonly Color Accent = Color.FromArgb(121, 87, 213);
    static readonly Color AccentEdge = Color.FromArgb(140, 112, 232);

    public static void BuildIco(string path)
    {
        int[] sizes = { 16, 20, 24, 32, 40, 48, 64, 128, 256 };
        var frames = new List<byte[]>();
        foreach (int size in sizes) frames.Add(Render(size));

        using (var stream = File.Create(path))
        using (var writer = new BinaryWriter(stream))
        {
            writer.Write((short)0);
            writer.Write((short)1);
            writer.Write((short)frames.Count);
            int offset = 6 + 16 * frames.Count;
            for (int i = 0; i < frames.Count; i++)
            {
                int size = sizes[i];
                writer.Write((byte)(size >= 256 ? 0 : size));
                writer.Write((byte)(size >= 256 ? 0 : size));
                writer.Write((byte)0);
                writer.Write((byte)0);
                writer.Write((short)1);
                writer.Write((short)32);
                writer.Write(frames[i].Length);
                writer.Write(offset);
                offset += frames[i].Length;
            }
            foreach (byte[] frame in frames) writer.Write(frame);
        }
    }

    public static void BuildPng(string path, int size)
    {
        using (var bitmap = new Bitmap(size, size, PixelFormat.Format32bppArgb))
        {
            using (Graphics g = Graphics.FromImage(bitmap))
            {
                g.SmoothingMode = SmoothingMode.AntiAlias;
                g.Clear(Color.Transparent);
                Draw(g, size);
            }
            bitmap.Save(path, ImageFormat.Png);
        }
    }

    static byte[] Render(int size)
    {
        int big = Math.Max(size * 4, 256);
        using (var large = new Bitmap(big, big, PixelFormat.Format32bppArgb))
        {
            using (Graphics g = Graphics.FromImage(large))
            {
                g.SmoothingMode = SmoothingMode.AntiAlias;
                g.Clear(Color.Transparent);
                Draw(g, big);
            }
            using (var small = new Bitmap(size, size, PixelFormat.Format32bppArgb))
            {
                using (Graphics g = Graphics.FromImage(small))
                {
                    g.InterpolationMode = InterpolationMode.HighQualityBicubic;
                    g.PixelOffsetMode = PixelOffsetMode.HighQuality;
                    g.DrawImage(large, new Rectangle(0, 0, size, size));
                }
                using (var buffer = new MemoryStream())
                {
                    small.Save(buffer, ImageFormat.Png);
                    return buffer.ToArray();
                }
            }
        }
    }

    static void Draw(Graphics g, int size)
    {
        float s = size / 256f;
        float margin = 10f * s;
        var tile = new RectangleF(margin, margin, size - margin * 2f, size - margin * 2f);
        float radius = 46f * s;

        using (GraphicsPath path = Round(tile, radius))
        using (var brush = new SolidBrush(Accent))
        {
            g.FillPath(brush, path);
        }
        using (GraphicsPath path = Round(tile, radius))
        using (var pen = new Pen(Color.FromArgb(90, AccentEdge), Math.Max(1f, 2f * s)))
        {
            g.DrawPath(pen, path);
        }

        float cx = size * 0.5f;
        float cy = size * 0.5f;
        float w = size * 0.20f;
        float h = size * 0.26f;
        var glyph = new[]
        {
            new PointF(cx - w * 0.45f, cy - h / 2f),
            new PointF(cx - w * 0.45f, cy + h / 2f),
            new PointF(cx + w * 0.70f, cy),
        };
        using (var brush = new SolidBrush(Color.White))
        {
            g.FillPolygon(brush, glyph);
        }
    }

    static GraphicsPath Round(RectangleF bounds, float radius)
    {
        var path = new GraphicsPath();
        float d = radius * 2f;
        var arc = new RectangleF(bounds.X, bounds.Y, d, d);
        path.AddArc(arc, 180, 90);
        arc.X = bounds.Right - d;
        path.AddArc(arc, 270, 90);
        arc.Y = bounds.Bottom - d;
        path.AddArc(arc, 0, 90);
        arc.X = bounds.X;
        path.AddArc(arc, 90, 90);
        path.CloseFigure();
        return path;
    }
}
'@

[DhakaFlixAssets]::BuildIco((Join-Path $OutputDirectory 'app.ico'))
[DhakaFlixAssets]::BuildPng((Join-Path $OutputDirectory 'tray.png'), 32)
[DhakaFlixAssets]::BuildPng((Join-Path $OutputDirectory 'icon.png'), 256)
Write-Host "Assets written to $OutputDirectory"
