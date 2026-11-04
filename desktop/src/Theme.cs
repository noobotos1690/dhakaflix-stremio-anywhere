using System;
using System.Drawing;
using System.Drawing.Drawing2D;
using System.Drawing.Text;
using System.Windows.Forms;

namespace DhakaFlixRelay
{
    /// <summary>
    /// Colour, typography and vector primitives shared by every custom control.
    /// Palette mirrors the relay landing page: deep indigo canvas, indigo
    /// structure, one green call to action.
    /// </summary>
    internal static class Theme
    {
        public static readonly Color Bg = Color.FromArgb(10, 10, 24);
        public static readonly Color BgSoft = Color.FromArgb(13, 13, 30);
        public static readonly Color Card = Color.FromArgb(19, 19, 40);
        public static readonly Color CardSoft = Color.FromArgb(24, 24, 48);
        public static readonly Color CardRaised = Color.FromArgb(28, 28, 56);
        public static readonly Color Border = Color.FromArgb(39, 37, 76);
        public static readonly Color BorderSoft = Color.FromArgb(31, 30, 62);
        public static readonly Color Text = Color.FromArgb(248, 250, 252);
        public static readonly Color TextMuted = Color.FromArgb(170, 178, 197);
        public static readonly Color TextDim = Color.FromArgb(129, 139, 161);
        public static readonly Color Accent = Color.FromArgb(34, 197, 94);
        public static readonly Color AccentHover = Color.FromArgb(46, 213, 113);
        public static readonly Color AccentPressed = Color.FromArgb(22, 163, 74);
        public static readonly Color OnAccent = Color.FromArgb(4, 28, 14);
        public static readonly Color Indigo = Color.FromArgb(99, 102, 241);
        public static readonly Color IndigoSoft = Color.FromArgb(146, 156, 255);
        public static readonly Color IndigoDeep = Color.FromArgb(67, 56, 202);
        public static readonly Color Warning = Color.FromArgb(245, 158, 11);
        public static readonly Color WarningSoft = Color.FromArgb(251, 191, 36);
        public static readonly Color Danger = Color.FromArgb(248, 113, 113);
        public static readonly Color DangerDeep = Color.FromArgb(239, 68, 68);
        public static readonly Color Info = Color.FromArgb(125, 211, 252);
        public static readonly Color ChipBg = Color.FromArgb(29, 29, 57);
        public static readonly Color Track = Color.FromArgb(36, 35, 68);

        public static readonly Font FontH1;
        public static readonly Font FontStatus;
        public static readonly Font FontCardTitle;
        public static readonly Font FontBody;
        public static readonly Font FontBodyStrong;
        public static readonly Font FontSmall;
        public static readonly Font FontTiny;
        public static readonly Font FontButton;
        public static readonly Font FontMono;

        static Theme()
        {
            FontH1 = new Font("Segoe UI", 15.5f, FontStyle.Bold, GraphicsUnit.Point);
            FontStatus = new Font("Segoe UI", 16.5f, FontStyle.Bold, GraphicsUnit.Point);
            FontCardTitle = new Font("Segoe UI", 9.75f, FontStyle.Bold, GraphicsUnit.Point);
            FontBody = new Font("Segoe UI", 9f, FontStyle.Regular, GraphicsUnit.Point);
            FontBodyStrong = new Font("Segoe UI", 9f, FontStyle.Bold, GraphicsUnit.Point);
            FontSmall = new Font("Segoe UI", 8.25f, FontStyle.Regular, GraphicsUnit.Point);
            FontTiny = new Font("Segoe UI", 7.5f, FontStyle.Bold, GraphicsUnit.Point);
            FontButton = new Font("Segoe UI", 9f, FontStyle.Bold, GraphicsUnit.Point);
            FontMono = new Font(PickMonoFamily(), 8.5f, FontStyle.Regular, GraphicsUnit.Point);
        }

        private static string PickMonoFamily()
        {
            string[] candidates = { "Cascadia Mono", "Cascadia Code", "Consolas", "Lucida Console", "Courier New" };
            try
            {
                using (var installed = new InstalledFontCollection())
                {
                    foreach (string candidate in candidates)
                    {
                        foreach (FontFamily family in installed.Families)
                        {
                            if (string.Equals(family.Name, candidate, StringComparison.OrdinalIgnoreCase))
                                return candidate;
                        }
                    }
                }
            }
            catch
            {
                // fall through to a font Windows always ships
            }
            return "Consolas";
        }

        public static int Px(Control control, double value)
        {
            double scale = control == null ? 1d : control.DeviceDpi / 96d;
            return (int)Math.Round(value * scale);
        }

        public static float Pxf(Control control, double value)
        {
            double scale = control == null ? 1d : control.DeviceDpi / 96d;
            return (float)(value * scale);
        }

        public static GraphicsPath Rounded(RectangleF bounds, float radius)
        {
            var path = new GraphicsPath();
            float diameter = Math.Max(1f, radius * 2f);
            if (radius <= 0.5f)
            {
                path.AddRectangle(bounds);
                return path;
            }

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

        public static void FillRounded(Graphics g, RectangleF bounds, float radius, Color color)
        {
            using (GraphicsPath path = Rounded(bounds, radius))
            using (var brush = new SolidBrush(color))
            {
                g.FillPath(brush, path);
            }
        }

        public static void StrokeRounded(Graphics g, RectangleF bounds, float radius, Color color, float width)
        {
            using (GraphicsPath path = Rounded(bounds, radius))
            using (var pen = new Pen(color, width))
            {
                g.DrawPath(pen, path);
            }
        }

        public static void FillRoundedGradient(Graphics g, RectangleF bounds, float radius, Color from, Color to, float angle)
        {
            using (GraphicsPath path = Rounded(bounds, radius))
            using (var brush = new LinearGradientBrush(bounds, from, to, angle))
            {
                g.FillPath(brush, path);
            }
        }

        public static void DrawText(Graphics g, string text, Font font, Color color, Rectangle bounds, TextFormatFlags flags)
        {
            TextRenderer.DrawText(g, text ?? string.Empty, font, bounds, color, flags);
        }

        public static Size MeasureText(string text, Font font)
        {
            return TextRenderer.MeasureText(text ?? string.Empty, font, new Size(int.MaxValue, int.MaxValue),
                TextFormatFlags.NoPadding | TextFormatFlags.SingleLine | TextFormatFlags.NoPrefix);
        }

        public static Color Blend(Color back, Color front, double alpha)
        {
            alpha = Math.Max(0d, Math.Min(1d, alpha));
            return Color.FromArgb(
                (int)Math.Round(back.A + (front.A - back.A) * alpha),
                (int)Math.Round(back.R + (front.R - back.R) * alpha),
                (int)Math.Round(back.G + (front.G - back.G) * alpha),
                (int)Math.Round(back.B + (front.B - back.B) * alpha));
        }

        public static Color Alpha(Color color, int alpha)
        {
            return Color.FromArgb(Math.Max(0, Math.Min(255, alpha)), color);
        }

        public static string Ellipsize(string text, Font font, int maxWidth)
        {
            if (string.IsNullOrEmpty(text)) return string.Empty;
            if (MeasureText(text, font).Width <= maxWidth) return text;

            const string tail = "\u2026";
            int low = 0;
            int high = text.Length;
            while (low < high)
            {
                int mid = (low + high + 1) / 2;
                if (MeasureText(text.Substring(0, mid) + tail, font).Width <= maxWidth) low = mid;
                else high = mid - 1;
            }
            return low <= 0 ? tail : text.Substring(0, low) + tail;
        }
    }

    internal delegate void IconPainter(Graphics g, RectangleF bounds, Color color);

    /// <summary>Small vector glyphs, drawn on the same 16px grid as the type.</summary>
    internal static class Icons
    {
        public static void Play(Graphics g, RectangleF r, Color color)
        {
            float w = r.Width;
            float h = r.Height;
            var path = new GraphicsPath();
            path.AddPolygon(new[]
            {
                new PointF(r.X + w * 0.18f, r.Y + h * 0.10f),
                new PointF(r.X + w * 0.18f, r.Y + h * 0.90f),
                new PointF(r.X + w * 0.90f, r.Y + h * 0.50f),
            });
            path.CloseFigure();
            using (var brush = new SolidBrush(color)) g.FillPath(brush, path);
            path.Dispose();
        }

        public static void Stop(Graphics g, RectangleF r, Color color)
        {
            var box = new RectangleF(r.X + r.Width * 0.16f, r.Y + r.Height * 0.16f, r.Width * 0.68f, r.Height * 0.68f);
            using (GraphicsPath path = Theme.Rounded(box, r.Width * 0.16f))
            using (var brush = new SolidBrush(color)) g.FillPath(brush, path);
        }

        public static void Copy(Graphics g, RectangleF r, Color color)
        {
            using (var pen = new Pen(color, Math.Max(1.4f, r.Width / 11f)))
            {
                var back = new RectangleF(r.X + r.Width * 0.10f, r.Y + r.Height * 0.10f, r.Width * 0.58f, r.Height * 0.58f);
                var front = new RectangleF(r.X + r.Width * 0.32f, r.Y + r.Height * 0.32f, r.Width * 0.58f, r.Height * 0.58f);
                using (GraphicsPath path = Theme.Rounded(back, r.Width * 0.12f)) g.DrawPath(pen, path);
                using (GraphicsPath path = Theme.Rounded(front, r.Width * 0.12f)) g.DrawPath(pen, path);
            }
        }

        public static void External(Graphics g, RectangleF r, Color color)
        {
            using (var pen = new Pen(color, Math.Max(1.4f, r.Width / 11f)))
            {
                float pad = r.Width * 0.14f;
                var box = new RectangleF(r.X + pad, r.Y + r.Height * 0.30f, r.Width * 0.56f, r.Height * 0.56f);
                using (GraphicsPath path = Theme.Rounded(box, r.Width * 0.10f)) g.DrawPath(pen, path);
                g.DrawLine(pen, r.X + r.Width * 0.44f, r.Y + r.Height * 0.44f, r.Right - pad, r.Y + pad);
                g.DrawLine(pen, r.Right - pad - r.Width * 0.30f, r.Y + pad, r.Right - pad, r.Y + pad);
                g.DrawLine(pen, r.Right - pad, r.Y + pad, r.Right - pad, r.Y + pad + r.Height * 0.30f);
            }
        }

        public static void Terminal(Graphics g, RectangleF r, Color color)
        {
            using (var pen = new Pen(color, Math.Max(1.4f, r.Width / 11f)))
            {
                pen.StartCap = LineCap.Round;
                pen.EndCap = LineCap.Round;
                pen.LineJoin = LineJoin.Round;
                g.DrawLines(pen, new[]
                {
                    new PointF(r.X + r.Width * 0.18f, r.Y + r.Height * 0.26f),
                    new PointF(r.X + r.Width * 0.40f, r.Y + r.Height * 0.48f),
                    new PointF(r.X + r.Width * 0.18f, r.Y + r.Height * 0.70f),
                });
                g.DrawLine(pen, r.X + r.Width * 0.52f, r.Y + r.Height * 0.74f, r.X + r.Width * 0.84f, r.Y + r.Height * 0.74f);
            }
        }

        public static void Folder(Graphics g, RectangleF r, Color color)
        {
            using (var pen = new Pen(color, Math.Max(1.4f, r.Width / 11f)))
            {
                pen.LineJoin = LineJoin.Round;
                var p = new GraphicsPath();
                p.AddLines(new[]
                {
                    new PointF(r.X + r.Width * 0.14f, r.Y + r.Height * 0.30f),
                    new PointF(r.X + r.Width * 0.42f, r.Y + r.Height * 0.30f),
                    new PointF(r.X + r.Width * 0.50f, r.Y + r.Height * 0.40f),
                    new PointF(r.X + r.Width * 0.86f, r.Y + r.Height * 0.40f),
                    new PointF(r.X + r.Width * 0.86f, r.Y + r.Height * 0.72f),
                    new PointF(r.X + r.Width * 0.14f, r.Y + r.Height * 0.72f),
                });
                p.CloseFigure();
                g.DrawPath(pen, p);
                p.Dispose();
            }
        }

        public static void Shield(Graphics g, RectangleF r, Color color)
        {
            using (var pen = new Pen(color, Math.Max(1.4f, r.Width / 11f)))
            {
                pen.LineJoin = LineJoin.Round;
                var p = new GraphicsPath();
                p.AddLines(new[]
                {
                    new PointF(r.X + r.Width * 0.5f, r.Y + r.Height * 0.10f),
                    new PointF(r.X + r.Width * 0.86f, r.Y + r.Height * 0.24f),
                    new PointF(r.X + r.Width * 0.86f, r.Y + r.Height * 0.52f),
                    new PointF(r.X + r.Width * 0.5f, r.Y + r.Height * 0.90f),
                    new PointF(r.X + r.Width * 0.14f, r.Y + r.Height * 0.52f),
                    new PointF(r.X + r.Width * 0.14f, r.Y + r.Height * 0.24f),
                });
                p.CloseFigure();
                g.DrawPath(pen, p);
                p.Dispose();
            }
        }

        public static void Refresh(Graphics g, RectangleF r, Color color)
        {
            using (var pen = new Pen(color, Math.Max(1.4f, r.Width / 11f)))
            {
                var box = new RectangleF(r.X + r.Width * 0.16f, r.Y + r.Height * 0.16f, r.Width * 0.68f, r.Height * 0.68f);
                g.DrawArc(pen, box, -40f, 280f);
                g.FillPolygon(new SolidBrush(color), new[]
                {
                    new PointF(r.X + r.Width * 0.62f, r.Y + r.Height * 0.02f),
                    new PointF(r.X + r.Width * 0.96f, r.Y + r.Height * 0.20f),
                    new PointF(r.X + r.Width * 0.62f, r.Y + r.Height * 0.26f),
                });
            }
        }

        public static void Power(Graphics g, RectangleF r, Color color)
        {
            using (var pen = new Pen(color, Math.Max(1.5f, r.Width / 10f)))
            {
                pen.StartCap = LineCap.Round;
                pen.EndCap = LineCap.Round;
                var box = new RectangleF(r.X + r.Width * 0.17f, r.Y + r.Height * 0.17f, r.Width * 0.66f, r.Height * 0.66f);
                g.DrawArc(pen, box, -55f, 290f);
                g.DrawLine(pen, r.X + r.Width * 0.5f, r.Y + r.Height * 0.08f, r.X + r.Width * 0.5f, r.Y + r.Height * 0.46f);
            }
        }

        public static void Check(Graphics g, RectangleF r, Color color)
        {
            using (var pen = new Pen(color, Math.Max(1.6f, r.Width / 10f)))
            {
                pen.StartCap = LineCap.Round;
                pen.EndCap = LineCap.Round;
                pen.LineJoin = LineJoin.Round;
                g.DrawLines(pen, new[]
                {
                    new PointF(r.X + r.Width * 0.18f, r.Y + r.Height * 0.52f),
                    new PointF(r.X + r.Width * 0.42f, r.Y + r.Height * 0.74f),
                    new PointF(r.X + r.Width * 0.84f, r.Y + r.Height * 0.26f),
                });
            }
        }

        public static void Alert(Graphics g, RectangleF r, Color color)
        {
            using (var pen = new Pen(color, Math.Max(1.5f, r.Width / 12f)))
            {
                pen.LineJoin = LineJoin.Round;
                var path = new GraphicsPath();
                path.AddPolygon(new[]
                {
                    new PointF(r.X + r.Width * 0.50f, r.Y + r.Height * 0.08f),
                    new PointF(r.X + r.Width * 0.94f, r.Y + r.Height * 0.90f),
                    new PointF(r.X + r.Width * 0.06f, r.Y + r.Height * 0.90f),
                });
                path.CloseFigure();
                g.DrawPath(pen, path);
                path.Dispose();

                pen.StartCap = LineCap.Round;
                pen.EndCap = LineCap.Round;
                g.DrawLine(pen, r.X + r.Width * 0.50f, r.Y + r.Height * 0.38f,
                    r.X + r.Width * 0.50f, r.Y + r.Height * 0.62f);
                using (var brush = new SolidBrush(color))
                {
                    g.FillEllipse(brush, r.X + r.Width * 0.50f - r.Width * 0.035f,
                        r.Y + r.Height * 0.69f, r.Width * 0.07f, r.Width * 0.07f);
                }
            }
        }

        public static void ChevronDown(Graphics g, RectangleF r, Color color)
        {
            using (var pen = new Pen(color, Math.Max(1.5f, r.Width / 10f)))
            {
                pen.StartCap = LineCap.Round;
                pen.EndCap = LineCap.Round;
                pen.LineJoin = LineJoin.Round;
                g.DrawLines(pen, new[]
                {
                    new PointF(r.X + r.Width * 0.24f, r.Y + r.Height * 0.40f),
                    new PointF(r.X + r.Width * 0.50f, r.Y + r.Height * 0.64f),
                    new PointF(r.X + r.Width * 0.76f, r.Y + r.Height * 0.40f),
                });
            }
        }

        public static void Globe(Graphics g, RectangleF r, Color color)
        {
            using (var pen = new Pen(color, Math.Max(1.4f, r.Width / 12f)))
            {
                var box = new RectangleF(r.X + r.Width * 0.12f, r.Y + r.Height * 0.12f, r.Width * 0.76f, r.Height * 0.76f);
                g.DrawEllipse(pen, box);
                g.DrawEllipse(pen, new RectangleF(box.X + box.Width * 0.33f, box.Y, box.Width * 0.34f, box.Height));
                g.DrawLine(pen, box.X, box.Y + box.Height * 0.5f, box.Right, box.Y + box.Height * 0.5f);
            }
        }
    }
}
