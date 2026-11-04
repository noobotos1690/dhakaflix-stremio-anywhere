using System;
using System.Drawing;
using System.Drawing.Drawing2D;
using System.Windows.Forms;

namespace DhakaFlixRelay
{
    /// <summary>
    /// Shown when quitting from the tray. The addon URL is regenerated on every
    /// start, so the user is reminded to remove it from Stremio first.
    /// </summary>
    internal sealed class QuitDialog : Form
    {
        private readonly string _manifestUrl;
        private readonly AppButton _openButton;
        private readonly AppButton _cancelButton;
        private readonly AppButton _quitButton;
        private readonly AppButton _copyButton;
        private readonly Label _linkLabel;
        private readonly Timer _copyTimer;

        public bool OpenStremioRequested { get; private set; }

        public QuitDialog(string manifestUrl)
        {
            _manifestUrl = manifestUrl ?? string.Empty;

            Text = "Quit DhakaFlix Relay";
            BackColor = Theme.Bg;
            ForeColor = Theme.Text;
            Font = Theme.FontBody;
            FormBorderStyle = FormBorderStyle.FixedDialog;
            MaximizeBox = false;
            MinimizeBox = false;
            ShowInTaskbar = false;
            StartPosition = FormStartPosition.CenterScreen;
            ClientSize = new Size(560, 430);
            DoubleBuffered = true;
            AutoScaleMode = AutoScaleMode.Dpi;
            AutoScaleDimensions = new SizeF(96F, 96F);
            try { Icon = Icon.ExtractAssociatedIcon(Application.ExecutablePath); } catch { }

            var card = new Card
            {
                FillColor = Theme.Card,
                Radius = 14,
            };
            card.SetBounds(20, 18, 520, 394);
            Controls.Add(card);

            var badge = new AlertBadge();
            badge.SetBounds(18, 18, 42, 42);
            card.Controls.Add(badge);

            Label title = MakeLabel("The addon will stop working", 13.5f, FontStyle.Bold, Theme.Text);
            title.SetBounds(74, 14, 428, 28);
            card.Controls.Add(title);

            Label subtitle = MakeLabel(
                "Stremio saves the addon URL, and this relay creates a new URL every start.",
                8.5f, FontStyle.Regular, Theme.TextMuted);
            subtitle.SetBounds(74, 42, 428, 34);
            card.Controls.Add(subtitle);

            Label body = MakeLabel(
                "Remove DhakaFlix from Stremio before quitting. If you do not, Stremio keeps an addon that points at a dead link.",
                9f, FontStyle.Regular, Theme.TextMuted);
            body.SetBounds(18, 92, 484, 48);
            card.Controls.Add(body);

            var linkPanel = new Card
            {
                FillColor = Theme.CardSoft,
                StrokeColor = Theme.BorderSoft,
                Radius = 9,
            };
            linkPanel.SetBounds(18, 150, 484, 38);
            card.Controls.Add(linkPanel);

            _linkLabel = MakeLabel(Theme.Ellipsize(_manifestUrl, Theme.FontMono, 420), 8.5f, FontStyle.Regular, Theme.Text, true);
            _linkLabel.BackColor = Theme.CardSoft;
            _linkLabel.SetBounds(10, 10, 428, 18);
            linkPanel.Controls.Add(_linkLabel);

            _copyButton = new AppButton
            {
                Kind = ButtonKind.Quiet,
                IconPainter = Icons.Copy,
                Backplate = Theme.CardSoft,
                AccessibleName = "Copy addon link",
                TabStop = false,
            };
            _copyButton.SetBounds(444, 4, 30, 30);
            _copyButton.Click += (s, e) => CopyLink();
            linkPanel.Controls.Add(_copyButton);

            Label stepsTitle = MakeLabel("Remove it in Stremio", 9f, FontStyle.Bold, Theme.Text);
            stepsTitle.SetBounds(18, 200, 300, 18);
            card.Controls.Add(stepsTitle);

            AddStep(card, 226, "1", "Open Stremio");
            AddStep(card, 250, "2", "Go to Addons, then Installed");
            AddStep(card, 274, "3", "Select DhakaFlix and remove it");

            Label tip = MakeLabel(
                "Open Stremio keeps the relay running so you can remove the addon. Quit and stop relay closes the app.",
                8f, FontStyle.Regular, Theme.TextDim);
            tip.SetBounds(18, 306, 484, 30);
            card.Controls.Add(tip);

            _openButton = new AppButton
            {
                Text = "Open Stremio",
                Kind = ButtonKind.Secondary,
                IconPainter = Icons.Globe,
                Backplate = Theme.Card,
            };
            _openButton.SetBounds(74, 334, 150, 42);
            _openButton.Click += (s, e) =>
            {
                OpenStremioRequested = true;
                DialogResult = DialogResult.Cancel;
                Close();
            };
            card.Controls.Add(_openButton);

            _cancelButton = new AppButton
            {
                Text = "Cancel",
                Kind = ButtonKind.Ghost,
                Backplate = Theme.Card,
            };
            _cancelButton.SetBounds(232, 334, 92, 42);
            _cancelButton.Click += (s, e) =>
            {
                DialogResult = DialogResult.No;
                Close();
            };
            card.Controls.Add(_cancelButton);

            _quitButton = new AppButton
            {
                Text = "Quit and stop relay",
                Kind = ButtonKind.Danger,
                Backplate = Theme.Card,
            };
            _quitButton.SetBounds(332, 334, 170, 42);
            _quitButton.Click += (s, e) =>
            {
                DialogResult = DialogResult.Yes;
                Close();
            };
            card.Controls.Add(_quitButton);

            CancelButton = _cancelButton;
            ActiveControl = _cancelButton;

            _copyTimer = new Timer { Interval = 1600 };
            _copyTimer.Tick += (s, e) =>
            {
                _copyTimer.Stop();
                _copyButton.Text = string.Empty;
            };

            FormClosing += (s, e) =>
            {
                if (DialogResult == DialogResult.None) DialogResult = DialogResult.No;
            };
        }

        protected override void OnHandleCreated(EventArgs e)
        {
            base.OnHandleCreated(e);
            UiChrome.EnableDarkTitleBar(this);
        }

        protected override void OnShown(EventArgs e)
        {
            base.OnShown(e);
            _cancelButton.Focus();
        }

        private static void AddStep(Card card, int y, string number, string text)
        {
            Label index = MakeLabel(number, 8.5f, FontStyle.Bold, Theme.Accent);
            index.TextAlign = ContentAlignment.MiddleCenter;
            index.SetBounds(18, y, 18, 20);
            card.Controls.Add(index);

            Label label = MakeLabel(text, 9f, FontStyle.Regular, Theme.Text);
            label.SetBounds(42, y, 440, 20);
            card.Controls.Add(label);
        }

        private static Label MakeLabel(string text, float size, FontStyle style, Color color, bool mono = false)
        {
            return new Label
            {
                Text = text,
                Font = mono ? Theme.FontMono : new Font("Segoe UI", size, style, GraphicsUnit.Point),
                ForeColor = color,
                BackColor = Theme.Card,
                AutoSize = false,
                UseMnemonic = false,
                TextAlign = ContentAlignment.MiddleLeft,
                AutoEllipsis = false,
            };
        }

        private void CopyLink()
        {
            if (string.IsNullOrEmpty(_manifestUrl)) return;
            try
            {
                Clipboard.SetText(_manifestUrl);
                _copyButton.Text = "Copied";
                _copyTimer.Stop();
                _copyTimer.Start();
            }
            catch
            {
                // clipboard busy
            }
        }

        protected override void Dispose(bool disposing)
        {
            if (disposing) _copyTimer.Dispose();
            base.Dispose(disposing);
        }

        private sealed class AlertBadge : Control
        {
            public AlertBadge()
            {
                SetStyle(ControlStyles.AllPaintingInWmPaint | ControlStyles.OptimizedDoubleBuffer |
                         ControlStyles.UserPaint | ControlStyles.ResizeRedraw, true);
            }

            protected override void OnPaint(PaintEventArgs e)
            {
                Graphics g = e.Graphics;
                g.SmoothingMode = SmoothingMode.AntiAlias;
                g.Clear(Theme.Card);

                var circle = new RectangleF(0.5f, 0.5f, Width - 1f, Height - 1f);
                using (var brush = new SolidBrush(Color.FromArgb(42, 248, 113, 113)))
                {
                    g.FillEllipse(brush, circle);
                }
                using (var pen = new Pen(Color.FromArgb(110, 248, 113, 113), 1f))
                {
                    g.DrawEllipse(pen, circle);
                }
                Icons.Alert(g, new RectangleF(11, 10, 20, 20), Theme.Danger);
            }
        }
    }
}
