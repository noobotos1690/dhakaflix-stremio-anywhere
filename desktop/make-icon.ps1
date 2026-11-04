<#
.SYNOPSIS
    Draws desktop\assets\app.ico.

.DESCRIPTION
    Run with Windows PowerShell (5.1), which ships the classic
    System.Drawing / GDI+ stack this script needs:

        powershell.exe -NoProfile -ExecutionPolicy Bypass -File make-icon.ps1

    build.ps1 does this automatically.
#>

[CmdletBinding()]
param(
    [string]$Path,
    [switch]$Force
)

$ErrorActionPreference = 'Stop'
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
if (-not $Path) { $Path = Join-Path $here 'assets\app.ico' }
if ((Test-Path $Path) -and -not $Force) {
    Write-Host "Icon already exists: $Path"
    exit 0
}

Add-Type -ReferencedAssemblies 'System.Drawing' -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Drawing;
using System.Drawing.Drawing2D;
using System.Drawing.Imaging;
using System.IO;

public static class DhakaFlixIcon
{
    public static void Build(string path)
    {
        int[] sizes = { 16, 24, 32, 48, 64, 128, 256 };
        var frames = new List<byte[]>();
        var widths = new List<int>();

        foreach (int size in sizes)
        {
            widths.Add(size);
            frames.Add(Render(size));
        }

        using (var stream = File.Create(path))
        using (var writer = new BinaryWriter(stream))
        {
            writer.Write((short)0);
            writer.Write((short)1);
            writer.Write((short)frames.Count);

            int offset = 6 + 16 * frames.Count;
            for (int i = 0; i < frames.Count; i++)
            {
                int size = widths[i];
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

    private static byte[] Render(int size)
    {
        int big = Math.Max(size * 4, 128);
        using (var large = new Bitmap(big, big, PixelFormat.Format32bppArgb))
        {
            using (Graphics g = Graphics.FromImage(large))
            {
                g.SmoothingMode = SmoothingMode.AntiAlias;
                g.InterpolationMode = InterpolationMode.HighQualityBicubic;
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

    private static void Draw(Graphics g, int size)
    {
        float s = size / 256f;
        float margin = 8f * s;
        var tile = new RectangleF(margin, margin, size - margin * 2f, size - margin * 2f);
        float radius = 56f * s;

        using (GraphicsPath path = Round(tile, radius))
        using (var brush = new LinearGradientBrush(tile, Color.FromArgb(67, 56, 202), Color.FromArgb(124, 58, 237), 45f))
        {
            g.FillPath(brush, path);
        }

        using (GraphicsPath path = Round(tile, radius))
        using (var pen = new Pen(Color.FromArgb(70, 199, 210, 254), Math.Max(1f, 3f * s)))
        {
            g.DrawPath(pen, path);
        }

        var shine = new RectangleF(tile.X + tile.Width * 0.12f, tile.Y + tile.Height * 0.08f,
            tile.Width * 0.76f, tile.Height * 0.40f);
        using (var brush = new SolidBrush(Color.FromArgb(30, 255, 255, 255)))
        {
            g.FillEllipse(brush, shine);
        }

        float cx = size * 0.50f;
        float cy = size * 0.50f;
        float w = size * 0.26f;
        float h = size * 0.32f;
        var triangle = new[]
        {
            new PointF(cx - w * 0.42f + 2f * s, cy - h / 2f),
            new PointF(cx - w * 0.42f + 2f * s, cy + h / 2f),
            new PointF(cx + w * 0.62f + 2f * s, cy),
        };
        using (var brush = new SolidBrush(Color.White))
        {
            g.FillPolygon(brush, triangle);
        }

        float dot = 40f * s;
        float dx = size - margin - dot - 6f * s;
        float dy = size - margin - dot - 6f * s;
        using (var brush = new SolidBrush(Color.FromArgb(79, 70, 229)))
        {
            g.FillEllipse(brush, dx - 8f * s, dy - 8f * s, dot + 16f * s, dot + 16f * s);
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

New-Item -ItemType Directory -Force -Path (Split-Path -Parent $Path) | Out-Null
[DhakaFlixIcon]::Build($Path)
Write-Host "Icon written: $Path"
