using System;
using System.Collections.Generic;
using System.Drawing;
using System.Drawing.Drawing2D;
using System.Windows.Forms;

namespace DhakaFlixRelay
{
    /// <summary>A rounded surface used for every panel in the window.</summary>
    internal sealed class Card : Panel
    {
        private Color _fillColor = Theme.Card;

        public Color FillColor
        {
            get { return _fillColor; }
            set
            {
                _fillColor = value;
                BackColor = value;
                Invalidate();
            }
        }

        public Color StrokeColor = Theme.Border;
        public float Radius = 14f;

        public Card()
        {
            SetStyle(ControlStyles.AllPaintingInWmPaint | ControlStyles.OptimizedDoubleBuffer |
                     ControlStyles.UserPaint | ControlStyles.ResizeRedraw, true);
            BackColor = Theme.Card;
        }

        protected override void OnPaint(PaintEventArgs e)
        {
            Graphics g = e.Graphics;
            g.SmoothingMode = SmoothingMode.AntiAlias;
            g.Clear(Theme.Bg);

            float radius = Theme.Pxf(this, Radius);
            var bounds = new RectangleF(0.5f, 0.5f, Width - 1f, Height - 1f);
            Theme.FillRounded(g, bounds, radius, FillColor);
            using (GraphicsPath path = Theme.Rounded(bounds, radius))
            using (var pen = new Pen(StrokeColor, 1f))
            {
                g.DrawPath(pen, path);
            }
            base.OnPaint(e);
        }
    }

    internal enum ButtonKind
    {
        Primary,
        Secondary,
        Ghost,
        Quiet,
        Danger,
        Link
    }

    /// <summary>Owner-drawn button with the four visual roles the window needs.</summary>
    internal sealed class AppButton : Button
    {
        private bool _hover;
        private bool _down;

        public ButtonKind Kind = ButtonKind.Secondary;
        public IconPainter IconPainter;
        public int IconSize = 16;
        public float Radius = 10f;
        public Color Backplate = Theme.Card;

        public AppButton()
        {
            SetStyle(ControlStyles.AllPaintingInWmPaint | ControlStyles.OptimizedDoubleBuffer |
                     ControlStyles.UserPaint | ControlStyles.ResizeRedraw, true);
            FlatStyle = FlatStyle.Flat;
            FlatAppearance.BorderSize = 0;
            UseVisualStyleBackColor = false;
            BackColor = Theme.Card;
            ForeColor = Theme.Text;
            Font = Theme.FontButton;
            Cursor = Cursors.Hand;
            Height = 40;
            TabStop = true;
        }

        protected override void OnMouseEnter(EventArgs e)
        {
            _hover = true;
            Invalidate();
            base.OnMouseEnter(e);
        }

        protected override void OnMouseLeave(EventArgs e)
        {
            _hover = false;
            _down = false;
            Invalidate();
            base.OnMouseLeave(e);
        }

        protected override void OnMouseDown(MouseEventArgs e)
        {
            if (e.Button == MouseButtons.Left) _down = true;
            Invalidate();
            base.OnMouseDown(e);
        }

        protected override void OnMouseUp(MouseEventArgs e)
        {
            _down = false;
            Invalidate();
            base.OnMouseUp(e);
        }

        protected override void OnEnabledChanged(EventArgs e)
        {
            Cursor = Enabled ? Cursors.Hand : Cursors.Default;
            Invalidate();
            base.OnEnabledChanged(e);
        }

        protected override void OnGotFocus(EventArgs e)
        {
            Invalidate();
            base.OnGotFocus(e);
        }

        protected override void OnLostFocus(EventArgs e)
        {
            Invalidate();
            base.OnLostFocus(e);
        }

        protected override void OnPaint(PaintEventArgs e)
        {
            Graphics g = e.Graphics;
            g.SmoothingMode = SmoothingMode.AntiAlias;
            g.Clear(Backplate);

            float radius = Theme.Pxf(this, Radius);
            var bounds = new RectangleF(0.5f, 0.5f, Width - 1f, Height - 1f);

            Color fill;
            Color border;
            Color textColor;

            switch (Kind)
            {
                case ButtonKind.Primary:
                    if (!Enabled)
                    {
                        fill = Theme.CardSoft;
                        border = Theme.Border;
                        textColor = Theme.TextDim;
                    }
                    else
                    {
                        fill = _down ? Theme.AccentPressed : _hover ? Theme.AccentHover : Theme.Accent;
                        border = Color.Transparent;
                        textColor = Theme.OnAccent;
                    }
                    break;
                case ButtonKind.Danger:
                    fill = _down ? Color.FromArgb(90, 239, 68, 68) : _hover ? Color.FromArgb(65, 239, 68, 68) : Color.FromArgb(40, 239, 68, 68);
                    border = Color.FromArgb(110, 239, 68, 68);
                    textColor = Enabled ? Theme.Danger : Theme.TextDim;
                    break;
                case ButtonKind.Ghost:
                    fill = _down ? Color.FromArgb(46, 99, 102, 241) : _hover ? Color.FromArgb(30, 99, 102, 241) : Color.Transparent;
                    border = _hover ? Color.FromArgb(120, 99, 102, 241) : Theme.Border;
                    textColor = Enabled ? Theme.Text : Theme.TextDim;
                    break;
                case ButtonKind.Quiet:
                    fill = _down ? Color.FromArgb(50, 120, 130, 160) : _hover ? Color.FromArgb(26, 140, 150, 190) : Color.Transparent;
                    border = Color.Transparent;
                    textColor = Enabled ? Theme.TextMuted : Theme.TextDim;
                    break;
                case ButtonKind.Link:
                    fill = Color.Transparent;
                    border = Color.Transparent;
                    textColor = Enabled ? Theme.IndigoSoft : Theme.TextDim;
                    break;
                default:
                    fill = _down ? Theme.CardRaised : _hover ? Color.FromArgb(34, 34, 66) : Theme.CardSoft;
                    border = _hover ? Color.FromArgb(90, 99, 102, 241) : Theme.Border;
                    textColor = Enabled ? Theme.Text : Theme.TextDim;
                    break;
            }

            if (Kind != ButtonKind.Link && Kind != ButtonKind.Quiet)
            {
                if (fill.A > 0)
                {
                    Theme.FillRounded(g, bounds, radius, fill);
                    using (GraphicsPath path = Theme.Rounded(bounds, radius))
                    using (var pen = new Pen(border, 1f))
                    {
                        g.DrawPath(pen, path);
                    }
                }
            }
            else if (_hover || _down)
            {
                Theme.FillRounded(g, bounds, radius, fill);
            }

            var textFlags = TextFormatFlags.NoPadding | TextFormatFlags.SingleLine | TextFormatFlags.NoPrefix |
                            TextFormatFlags.VerticalCenter | TextFormatFlags.HorizontalCenter;
            Size textSize = Theme.MeasureText(Text, Font);
            int iconSize = Theme.Px(this, IconSize);
            int gap = IconPainter != null && !string.IsNullOrEmpty(Text) ? Theme.Px(this, 8) : 0;
            int total = (IconPainter != null ? iconSize : 0) + gap + textSize.Width;
            int left = (Width - total) / 2;

            if (IconPainter != null)
            {
                var iconRect = new RectangleF(left, (Height - iconSize) / 2f, iconSize, iconSize);
                IconPainter(g, iconRect, textColor);
                left += iconSize + gap;
            }

            var textRect = new Rectangle(left, 0, textSize.Width + Theme.Px(this, 4), Height);
            Theme.DrawText(g, Text, Font, textColor, textRect, textFlags);

            if (Focused && ShowFocusCues && Enabled)
            {
                var ring = new RectangleF(bounds.X + 2f, bounds.Y + 2f, bounds.Width - 4f, bounds.Height - 4f);
                Theme.StrokeRounded(g, ring, radius - 1f, Theme.Alpha(Theme.IndigoSoft, 150), 1.4f);
            }
        }
    }

    /// <summary>Animated pill switch with keyboard support.</summary>
    internal sealed class ToggleSwitch : Control
    {
        private bool _checked;
        private float _position;
        private readonly Timer _animation;

        public event EventHandler CheckedChanged;

        public ToggleSwitch()
        {
            SetStyle(ControlStyles.AllPaintingInWmPaint | ControlStyles.OptimizedDoubleBuffer |
                     ControlStyles.UserPaint | ControlStyles.ResizeRedraw | ControlStyles.Selectable, true);
            Size = new Size(38, 21);
            Cursor = Cursors.Hand;
            TabStop = true;
            AccessibleRole = AccessibleRole.CheckButton;

            _animation = new Timer { Interval = 15 };
            _animation.Tick += (s, e) =>
            {
                float target = _checked ? 1f : 0f;
                float delta = target - _position;
                if (Math.Abs(delta) < 0.06f)
                {
                    _position = target;
                    _animation.Stop();
                }
                else
                {
                    _position += delta * 0.35f;
                }
                Invalidate();
            };
        }

        public bool Checked
        {
            get { return _checked; }
            set
            {
                if (_checked == value) return;
                _checked = value;
                AccessibleDefaultActionDescription = value ? "On" : "Off";
                _animation.Start();
                Invalidate();
                EventHandler handler = CheckedChanged;
                if (handler != null) handler(this, EventArgs.Empty);
            }
        }

        public void SetCheckedSilently(bool value)
        {
            _checked = value;
            _position = value ? 1f : 0f;
            AccessibleDefaultActionDescription = value ? "On" : "Off";
            Invalidate();
        }

        protected override void OnClick(EventArgs e)
        {
            Checked = !Checked;
            Focus();
            base.OnClick(e);
        }

        protected override void OnKeyDown(KeyEventArgs e)
        {
            if (e.KeyCode == Keys.Space || e.KeyCode == Keys.Enter)
            {
                Checked = !Checked;
                e.Handled = true;
            }
            base.OnKeyDown(e);
        }

        protected override void OnGotFocus(EventArgs e)
        {
            Invalidate();
            base.OnGotFocus(e);
        }

        protected override void OnLostFocus(EventArgs e)
        {
            Invalidate();
            base.OnLostFocus(e);
        }

        protected override void OnPaint(PaintEventArgs e)
        {
            Graphics g = e.Graphics;
            g.SmoothingMode = SmoothingMode.AntiAlias;
            g.Clear(Parent != null ? Parent.BackColor : Theme.Card);

            int h = Height;
            int w = Width;
            float radius = h / 2f;
            var track = new RectangleF(0.5f, 0.5f, w - 1f, h - 1f);

            Color trackColor = _checked
                ? Theme.Blend(Theme.Track, Theme.Accent, 0.35d + 0.65d * _position)
                : Theme.Track;
            Theme.FillRounded(g, track, radius, trackColor);
            using (GraphicsPath path = Theme.Rounded(track, radius))
            using (var pen = new Pen(_checked ? Theme.Alpha(Theme.Accent, 190) : Theme.Border, 1f))
            {
                g.DrawPath(pen, path);
            }

            float thumb = h - 2f * Theme.Pxf(this, 2.5f);
            float travel = w - thumb - Theme.Pxf(this, 5f);
            float x = Theme.Pxf(this, 2.5f) + travel * _position;
            float y = (h - thumb) / 2f;
            var thumbRect = new RectangleF(x, y, thumb, thumb);

            using (var brush = new SolidBrush(_checked ? Color.FromArgb(6, 40, 20) : Color.FromArgb(226, 232, 240)))
            {
                g.FillEllipse(brush, thumbRect);
            }

            if (Focused && ShowFocusCues)
            {
                Theme.StrokeRounded(g, new RectangleF(track.X - 2f, track.Y - 2f, track.Width + 4f, track.Height + 4f),
                    radius + 2f, Theme.Alpha(Theme.IndigoSoft, 150), 1.2f);
            }
        }

        protected override void Dispose(bool disposing)
        {
            if (disposing) _animation.Dispose();
            base.Dispose(disposing);
        }
    }

    /// <summary>Status chip: coloured dot, label, optional pulse for transitional states.</summary>
    internal sealed class StatusPill : Control
    {
        private readonly Timer _pulse;
        private float _phase;
        private Color _dotColor = Theme.TextDim;
        private string _label = "Stopped";

        public StatusPill()
        {
            SetStyle(ControlStyles.AllPaintingInWmPaint | ControlStyles.OptimizedDoubleBuffer |
                     ControlStyles.UserPaint | ControlStyles.ResizeRedraw, true);
            Size = new Size(132, 30);
            AccessibleRole = AccessibleRole.StaticText;

            _pulse = new Timer { Interval = 40 };
            _pulse.Tick += (s, e) =>
            {
                _phase += 0.09f;
                if (_phase > (float)(Math.PI * 2)) _phase -= (float)(Math.PI * 2);
                Invalidate();
            };
        }

        public void Set(string label, Color dotColor, bool pulsing)
        {
            _label = label;
            _dotColor = dotColor;
            Text = label;
            AccessibleName = "Relay status: " + label;
            if (pulsing && !_pulse.Enabled) _pulse.Start();
            if (!pulsing && _pulse.Enabled)
            {
                _pulse.Stop();
                _phase = 0f;
            }
            Invalidate();
        }

        protected override void OnPaint(PaintEventArgs e)
        {
            Graphics g = e.Graphics;
            g.SmoothingMode = SmoothingMode.AntiAlias;
            g.Clear(Parent != null ? Parent.BackColor : Theme.Bg);

            var bounds = new RectangleF(0.5f, 0.5f, Width - 1f, Height - 1f);
            Theme.FillRounded(g, bounds, Height / 2f, Theme.CardSoft);
            using (GraphicsPath path = Theme.Rounded(bounds, Height / 2f))
            using (var pen = new Pen(Theme.Border, 1f))
            {
                g.DrawPath(pen, path);
            }

            float dot = Theme.Pxf(this, 8f);
            float cx = Theme.Pxf(this, 14f) + dot / 2f;
            float cy = Height / 2f;

            float pulse = (float)((Math.Sin(_phase) + 1d) / 2d);
            if (_pulse.Enabled)
            {
                float ring = dot + Theme.Pxf(this, 4f) + pulse * Theme.Pxf(this, 4f);
                using (var brush = new SolidBrush(Theme.Alpha(_dotColor, (int)(70 * (1f - pulse) + 20))))
                {
                    g.FillEllipse(brush, cx - ring / 2f, cy - ring / 2f, ring, ring);
                }
            }

            using (var brush = new SolidBrush(_dotColor))
            {
                g.FillEllipse(brush, cx - dot / 2f, cy - dot / 2f, dot, dot);
            }

            var textRect = new Rectangle((int)(cx + dot / 2f + Theme.Pxf(this, 8f)), 0,
                Width - (int)(cx + dot / 2f + Theme.Pxf(this, 8f)) - Theme.Px(this, 8), Height);
            Theme.DrawText(g, _label, Theme.FontSmall, Theme.Text, textRect,
                TextFormatFlags.NoPadding | TextFormatFlags.SingleLine | TextFormatFlags.VerticalCenter |
                TextFormatFlags.NoPrefix | TextFormatFlags.Left);
        }

        protected override void Dispose(bool disposing)
        {
            if (disposing) _pulse.Dispose();
            base.Dispose(disposing);
        }
    }

    /// <summary>Row of small key/value chips ("PORT 7001", "MIRRORS 3/4").</summary>
    internal sealed class MetaChips : Control
    {
        private readonly List<KeyValuePair<string, string>> _items = new List<KeyValuePair<string, string>>();
        private int _lastHeight = 24;

        public MetaChips()
        {
            SetStyle(ControlStyles.AllPaintingInWmPaint | ControlStyles.OptimizedDoubleBuffer |
                     ControlStyles.UserPaint | ControlStyles.ResizeRedraw, true);
            Height = 24;
            AccessibleRole = AccessibleRole.StaticText;
        }

        public void SetItems(params KeyValuePair<string, string>[] items)
        {
            _items.Clear();
            _items.AddRange(items);
            Invalidate();
        }

        public string Describe()
        {
            var parts = new List<string>();
            foreach (var item in _items) parts.Add(item.Key + " " + item.Value);
            return string.Join(", ", parts.ToArray());
        }

        protected override void OnPaint(PaintEventArgs e)
        {
            Graphics g = e.Graphics;
            g.SmoothingMode = SmoothingMode.AntiAlias;
            g.Clear(Parent != null ? Parent.BackColor : Theme.Card);

            int padH = Theme.Px(this, 7);
            int gap = Theme.Px(this, 6);
            int h = Theme.Px(this, 24);
            int x = 0;
            _lastHeight = h;

            foreach (var item in _items)
            {
                Size keySize = Theme.MeasureText(item.Key.ToUpperInvariant(), Theme.FontTiny);
                Size valueSize = Theme.MeasureText(item.Value, Theme.FontSmall);
                int width = padH * 2 + keySize.Width + Theme.Px(this, 5) + valueSize.Width;
                if (x + width > Width) break;

                var chip = new RectangleF(x + 0.5f, 0.5f, width - 1f, h - 1f);
                Theme.FillRounded(g, chip, Theme.Pxf(this, 7f), Theme.ChipBg);

                var keyRect = new Rectangle(x + padH, 0, keySize.Width + 2, h);
                Theme.DrawText(g, item.Key.ToUpperInvariant(), Theme.FontTiny, Theme.TextDim, keyRect,
                    TextFormatFlags.NoPadding | TextFormatFlags.SingleLine | TextFormatFlags.VerticalCenter |
                    TextFormatFlags.NoPrefix | TextFormatFlags.Left);

                var valueRect = new Rectangle(x + padH + keySize.Width + Theme.Px(this, 5), 0, valueSize.Width + 4, h);
                Theme.DrawText(g, item.Value, Theme.FontSmall, Theme.Text, valueRect,
                    TextFormatFlags.NoPadding | TextFormatFlags.SingleLine | TextFormatFlags.VerticalCenter |
                    TextFormatFlags.NoPrefix | TextFormatFlags.Left);

                x += width + gap;
            }
        }
    }

    /// <summary>Thin progress track used by the setup banner.</summary>
    internal sealed class ThinProgress : Control
    {
        private readonly Timer _timer;
        private int _value;
        private bool _indeterminate;
        private float _offset;

        public ThinProgress()
        {
            SetStyle(ControlStyles.AllPaintingInWmPaint | ControlStyles.OptimizedDoubleBuffer |
                     ControlStyles.UserPaint | ControlStyles.ResizeRedraw, true);
            Height = 5;

            _timer = new Timer { Interval = 30 };
            _timer.Tick += (s, e) =>
            {
                _offset += 0.018f;
                if (_offset > 1f) _offset -= 1f;
                Invalidate();
            };
        }

        public int Value
        {
            get { return _value; }
            set
            {
                _value = Math.Max(0, Math.Min(100, value));
                Invalidate();
            }
        }

        public bool Indeterminate
        {
            get { return _indeterminate; }
            set
            {
                if (_indeterminate == value) return;
                _indeterminate = value;
                if (value) _timer.Start(); else _timer.Stop();
                Invalidate();
            }
        }

        protected override void OnPaint(PaintEventArgs e)
        {
            Graphics g = e.Graphics;
            g.SmoothingMode = SmoothingMode.AntiAlias;
            g.Clear(Parent != null ? Parent.BackColor : Theme.Card);

            float h = Theme.Pxf(this, 5f);
            var track = new RectangleF(0, (Height - h) / 2f, Width, h);
            Theme.FillRounded(g, track, h / 2f, Theme.Track);

            if (_indeterminate)
            {
                float segment = Math.Max(Theme.Pxf(this, 60f), Width * 0.22f);
                float x = -segment + (Width + segment) * _offset;
                var fill = new RectangleF(Math.Max(0, x), track.Y, Math.Min(segment, Math.Max(0, Width - x)), h);
                if (fill.Width > 1f)
                {
                    using (GraphicsPath path = Theme.Rounded(fill, h / 2f))
                    using (var brush = new LinearGradientBrush(fill, Theme.Indigo, Theme.Accent, 0f))
                    {
                        g.FillPath(brush, path);
                    }
                }
            }
            else if (_value > 0)
            {
                float w = Math.Max(h, Width * (_value / 100f));
                var fill = new RectangleF(0, track.Y, w, h);
                using (GraphicsPath path = Theme.Rounded(fill, h / 2f))
                using (var brush = new LinearGradientBrush(fill, Theme.IndigoDeep, Theme.Accent, 0f))
                {
                    g.FillPath(brush, path);
                }
            }
        }

        protected override void Dispose(bool disposing)
        {
            if (disposing) _timer.Dispose();
            base.Dispose(disposing);
        }
    }

    /// <summary>App mark: indigo gradient tile, play glyph, green status dot.</summary>
    internal sealed class BrandMark : Control
    {
        public BrandMark()
        {
            SetStyle(ControlStyles.AllPaintingInWmPaint | ControlStyles.OptimizedDoubleBuffer |
                     ControlStyles.UserPaint | ControlStyles.ResizeRedraw, true);
            Size = new Size(40, 40);
        }

        protected override void OnPaint(PaintEventArgs e)
        {
            Graphics g = e.Graphics;
            g.SmoothingMode = SmoothingMode.AntiAlias;

            var tile = new RectangleF(0.5f, 0.5f, Width - 1f, Height - 1f);
            float radius = Theme.Pxf(this, 11f);
            Theme.FillRoundedGradient(g, tile, radius, Color.FromArgb(79, 70, 229), Color.FromArgb(124, 58, 237), 45f);

            using (GraphicsPath path = Theme.Rounded(tile, radius))
            using (var pen = new Pen(Color.FromArgb(90, 165, 180, 252), 1f))
            {
                g.DrawPath(pen, path);
            }

            var highlight = new RectangleF(tile.X + tile.Width * 0.12f, tile.Y + tile.Height * 0.08f,
                tile.Width * 0.76f, tile.Height * 0.36f);
            using (var brush = new SolidBrush(Color.FromArgb(28, 255, 255, 255)))
            {
                g.FillEllipse(brush, highlight);
            }

            float glyph = Width * 0.34f;
            var iconRect = new RectangleF((Width - glyph) / 2f + Theme.Pxf(this, 1f), (Height - glyph) / 2f, glyph, glyph);
            Icons.Play(g, iconRect, Color.White);

            float dot = Theme.Pxf(this, 9f);
            var dotRect = new RectangleF(Width - dot - Theme.Pxf(this, 3f), Height - dot - Theme.Pxf(this, 3f), dot, dot);
            using (var ring = new SolidBrush(Color.FromArgb(79, 70, 229)))
            {
                g.FillEllipse(ring, dotRect.X - Theme.Pxf(this, 1.5f), dotRect.Y - Theme.Pxf(this, 1.5f),
                    dotRect.Width + Theme.Pxf(this, 3f), dotRect.Height + Theme.Pxf(this, 3f));
            }
            using (var brush = new SolidBrush(Theme.Accent))
            {
                g.FillEllipse(brush, dotRect);
            }
        }
    }

    /// <summary>Small Win32 helpers shared by the dialogs.</summary>
    internal static class UiChrome
    {
        [System.Runtime.InteropServices.DllImport("dwmapi.dll")]
        private static extern int DwmSetWindowAttribute(IntPtr hwnd, int attribute, ref int value, int size);

        public static void EnableDarkTitleBar(Form form)
        {
            try
            {
                int dark = 1;
                if (DwmSetWindowAttribute(form.Handle, 20, ref dark, sizeof(int)) != 0)
                    DwmSetWindowAttribute(form.Handle, 19, ref dark, sizeof(int));
            }
            catch
            {
                // older Windows keeps the light title bar
            }
        }
    }

    /// <summary>One-pixel divider that inherits the card background.</summary>
    internal sealed class Divider : Control
    {
        public Divider()
        {
            SetStyle(ControlStyles.AllPaintingInWmPaint | ControlStyles.OptimizedDoubleBuffer |
                     ControlStyles.UserPaint | ControlStyles.ResizeRedraw, true);
            Height = 1;
            TabStop = false;
        }

        protected override void OnPaint(PaintEventArgs e)
        {
            using (var pen = new Pen(Theme.BorderSoft))
            {
                e.Graphics.DrawLine(pen, 0, 0, Width, 0);
            }
        }
    }
}
