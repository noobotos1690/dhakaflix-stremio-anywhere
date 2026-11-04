using System;
using System.Collections.Concurrent;
using System.Collections.Generic;
using System.Diagnostics;
using System.Drawing;
using System.Drawing.Drawing2D;
using System.IO;
using System.Text;
using System.Text.RegularExpressions;
using System.Threading;
using System.Threading.Tasks;
using System.Windows.Forms;

namespace DhakaFlixRelay
{
    internal sealed class MainForm : Form
    {
        private static readonly Regex NodeLogLine = new Regex(
            @"^\[(\d{4}-\d{2}-\d{2}T[^\]]+)\]\s*\[(\w+)\]\s*\[([^\]]+)\]\s*(.*)$",
            RegexOptions.Compiled);

        private const string LinkPlaceholder = "Your addon link will appear here";

        private readonly AppPaths _paths;
        private readonly AppSettings _settings;
        private readonly RelayController _relay;
        private readonly DependencyManager _deps;
        private readonly bool _autostart;
        private float _scale;

        private DependencySnapshot _snapshot;
        private bool _installing;
        private bool _setupThenStart;
        private bool _autoStartPending;
        private bool _suppressToggle;
        private bool _hasLink;
        private bool _forceExit;
        private bool _quitting;
        private bool _trayHintShown;
        private bool _firstRunPromoted;
        private string _lastIssuedLink;
        private int _filterMode = 1; // 0 all, 1 info+, 2 warnings, 3 errors
        private int _logCount;
        private string _lastManifest;

        private readonly ConcurrentQueue<LogEntry> _pending = new ConcurrentQueue<LogEntry>();
        private readonly List<LogEntry> _logEntries = new List<LogEntry>();

        // header
        private BrandMark _brand;
        private Label _title;
        private Label _subtitle;
        private Label _version;
        private StatusPill _statusPill;
        private Card _banner;
        private Label _bannerTitle;
        private Label _bannerDetail;
        private Label _bannerPercent;
        private AppButton _setupButton;
        private ThinProgress _progress;

        // left column
        private Card _statusCard;
        private Label _statusLabel;
        private Label _statusTitle;
        private Label _statusDetail;
        private MetaChips _chips;
        private AppButton _startButton;

        private Card _prefsCard;
        private Label _prefsTitle;
        private Label _startupLabel;
        private Label _startupSub;
        private ToggleSwitch _startupToggle;
        private Label _autoStartLabel;
        private Label _autoStartSub;
        private ToggleSwitch _autoStartToggle;
        private Divider _prefsDivider;
        private Label _portLabel;
        private Label _portSub;
        private Card _portPanel;
        private TextBox _portBox;
        private AppButton _repairLink;
        private AppButton _adminLink;

        // right column
        private Card _installCard;
        private Label _installTitle;
        private AppButton _copyTopButton;
        private Card _urlPanel;
        private TextBox _urlBox;
        private AppButton _installButton;
        private AppButton _copyButton;
        private AppButton _openButton;
        private Label _urlHint;

        private Card _logsCard;
        private Label _logsTitle;
        private AppButton _filterButton;
        private Label _followLabel;
        private ToggleSwitch _followToggle;
        private AppButton _clearButton;
        private AppButton _folderButton;
        private Card _consolePanel;
        private RichTextBox _logView;
        private Label _logPath;
        private Label _logCountLabel;

        private readonly System.Windows.Forms.Timer _logTimer = new System.Windows.Forms.Timer();
        private readonly System.Windows.Forms.Timer _uptimeTimer = new System.Windows.Forms.Timer();
        private readonly System.Windows.Forms.Timer _healthTimer = new System.Windows.Forms.Timer();
        private readonly System.Windows.Forms.Timer _copyTimer = new System.Windows.Forms.Timer();
        private readonly object _logFileLock = new object();
        private StreamWriter _logWriter;
        private readonly ToolTip _tooltip = new ToolTip();

        private NotifyIcon _tray;
        private ContextMenuStrip _trayMenu;
        private ToolStripMenuItem _trayStart;
        private ToolStripMenuItem _trayStop;
        private ToolStripMenuItem _trayCopy;

        public MainForm(AppPaths paths, bool autostart)
        {
            _paths = paths;
            _autostart = autostart;
            _settings = AppSettings.Load();
            _scale = GetInitialScale();

            _relay = new RelayController(paths, _settings, (text, level) => PostLog(level, text));
            _deps = new DependencyManager(paths, (text, level) => PostLog(level, text));

            BuildWindow();
            BuildHeader();
            BuildBanner();
            BuildStatusCard();
            BuildPreferencesCard();
            BuildInstallCard();
            BuildLogCard();
            BuildTray();

            _relay.StateChanged += (s, e) => Post(() => ApplyRelayState(e.State, e.Detail));
            _relay.Log += OnRelayLog;
            _relay.PublicUrlChanged += manifest => Post(() => ApplyManifest(manifest));
            _relay.HealthUpdated += (s, e) => Post(() => ApplyHealth(e));

            _logTimer.Interval = 110;
            _logTimer.Tick += (s, e) => DrainLogs();
            _logTimer.Start();

            _uptimeTimer.Interval = 1000;
            _uptimeTimer.Tick += (s, e) => UpdateChips();
            _uptimeTimer.Start();

            _healthTimer.Interval = 4000;
            _healthTimer.Tick += (s, e) => PollHealth();

            _copyTimer.Interval = 1800;
            _copyTimer.Tick += (s, e) =>
            {
                _copyTimer.Stop();
                _copyTopButton.Text = string.Empty;
                _copyButton.Text = "Copy link";
            };

            if (autostart)
            {
                WindowState = FormWindowState.Minimized;
            }

            ApplyRelayState(RelayState.Stopped, "Relay is not running.");
        }

        // ------------------------------------------------------------------ build

        private float GetInitialScale()
        {
            try
            {
                using (Graphics g = Graphics.FromHwnd(IntPtr.Zero))
                {
                    return g.DpiX / 96f;
                }
            }
            catch
            {
                return 1f;
            }
        }

        private int S(double value)
        {
            return (int)Math.Round(value * _scale);
        }

        private void BuildWindow()
        {
            Text = "DhakaFlix Relay";
            BackColor = Theme.Bg;
            ForeColor = Theme.Text;
            Font = Theme.FontBody;
            AutoScaleMode = AutoScaleMode.None;
            FormBorderStyle = FormBorderStyle.Sizable;
            MaximizeBox = true;
            MinimizeBox = true;
            StartPosition = FormStartPosition.CenterScreen;
            ClientSize = new Size(S(1080), S(680));
            MinimumSize = new Size(S(940), S(690));
            DoubleBuffered = true;
            SetStyle(ControlStyles.OptimizedDoubleBuffer | ControlStyles.AllPaintingInWmPaint | ControlStyles.UserPaint, true);

            try { Icon = Icon.ExtractAssociatedIcon(Application.ExecutablePath); } catch { }

            DpiChanged += (s, e) =>
            {
                _scale = e.DeviceDpiNew / 96f;
                LayoutAll();
            };

            Load += (s, e) => OnLoaded();
            Shown += (s, e) => OnFirstShown();
            FormClosing += OnFormClosing;
        }

        private Label MakeLabel(string text, Font font, Color color)
        {
            return new Label
            {
                Text = text,
                Font = font,
                ForeColor = color,
                BackColor = Theme.Card,
                AutoSize = false,
                UseMnemonic = false,
                TextAlign = ContentAlignment.MiddleLeft,
                AutoEllipsis = false,
            };
        }

        private Label MakeMutedLabel(string text, Font font, Color color, Color back)
        {
            Label label = MakeLabel(text, font, color);
            label.BackColor = back;
            return label;
        }

        private AppButton MakeButton(string text, ButtonKind kind, IconPainter icon)
        {
            return new AppButton
            {
                Text = text,
                Kind = kind,
                IconPainter = icon,
                Backplate = Theme.Card,
            };
        }

        private void BuildHeader()
        {
            _brand = new BrandMark { BackColor = Theme.Bg };
            Controls.Add(_brand);

            _title = MakeLabel("DhakaFlix Relay", Theme.FontH1, Theme.Text);
            _title.BackColor = Theme.Bg;
            Controls.Add(_title);

            _subtitle = MakeLabel("Stremio addon relay for BDIX mirrors", Theme.FontSmall, Theme.TextMuted);
            _subtitle.BackColor = Theme.Bg;
            Controls.Add(_subtitle);

            _version = MakeLabel("v" + ShortVersion(), Theme.FontSmall, Theme.TextDim);
            _version.BackColor = Theme.Bg;
            _version.TextAlign = ContentAlignment.MiddleRight;
            Controls.Add(_version);

            _statusPill = new StatusPill();
            Controls.Add(_statusPill);
        }

        private static string ShortVersion()
        {
            string version = Application.ProductVersion;
            string[] parts = version.Split('.');
            if (parts.Length >= 3) return parts[0] + "." + parts[1] + "." + parts[2];
            return version;
        }

        private void BuildBanner()
        {
            _banner = new Card
            {
                FillColor = Color.FromArgb(36, 30, 20),
                StrokeColor = Color.FromArgb(92, 74, 34),
                Radius = 12,
                Visible = false,
            };
            Controls.Add(_banner);

            _bannerTitle = MakeLabel("Finish setup to start the relay", Theme.FontCardTitle, Color.FromArgb(253, 230, 180));
            _bannerTitle.BackColor = _banner.FillColor;
            _banner.Controls.Add(_bannerTitle);

            _bannerDetail = MakeLabel(string.Empty, Theme.FontSmall, Color.FromArgb(214, 197, 163));
            _bannerDetail.BackColor = _banner.FillColor;
            _bannerDetail.AutoEllipsis = true;
            _banner.Controls.Add(_bannerDetail);

            _bannerPercent = MakeLabel(string.Empty, Theme.FontSmall, Color.FromArgb(253, 230, 180));
            _bannerPercent.BackColor = _banner.FillColor;
            _bannerPercent.TextAlign = ContentAlignment.MiddleRight;
            _banner.Controls.Add(_bannerPercent);

            _setupButton = MakeButton("Install dependencies", ButtonKind.Primary, null);
            _setupButton.Backplate = _banner.FillColor;
            _setupButton.Click += (s, e) => BeginSetup(true);
            _banner.Controls.Add(_setupButton);

            _progress = new ThinProgress { Visible = false };
            _banner.Controls.Add(_progress);
        }

        private void SetBannerPalette(bool error)
        {
            Color fill = error ? Color.FromArgb(42, 24, 26) : Color.FromArgb(36, 30, 20);
            Color stroke = error ? Color.FromArgb(120, 50, 54) : Color.FromArgb(92, 74, 34);
            Color title = error ? Color.FromArgb(254, 202, 202) : Color.FromArgb(253, 230, 180);
            Color detail = error ? Color.FromArgb(238, 190, 190) : Color.FromArgb(214, 197, 163);

            _banner.FillColor = fill;
            _banner.StrokeColor = stroke;
            _banner.Invalidate();

            _bannerTitle.ForeColor = title;
            _bannerDetail.ForeColor = detail;
            _bannerPercent.ForeColor = title;
            _bannerTitle.BackColor = fill;
            _bannerDetail.BackColor = fill;
            _bannerPercent.BackColor = fill;

            _setupButton.Backplate = fill;
            _setupButton.Invalidate();
        }

        private void BuildStatusCard()
        {
            _statusCard = new Card();
            Controls.Add(_statusCard);

            _statusLabel = MakeLabel("RELAY STATUS", Theme.FontTiny, Theme.TextDim);
            _statusCard.Controls.Add(_statusLabel);

            _statusTitle = MakeLabel("Stopped", Theme.FontStatus, Theme.Text);
            _statusCard.Controls.Add(_statusTitle);

            _statusDetail = MakeLabel("Start the relay to get your Stremio install link.", Theme.FontSmall, Theme.TextMuted);
            _statusCard.Controls.Add(_statusDetail);

            _chips = new MetaChips { BackColor = Theme.Card };
            _chips.SetItems(
                new KeyValuePair<string, string>("Port", _settings.Port.ToString()),
                new KeyValuePair<string, string>("Uptime", "--:--"),
                new KeyValuePair<string, string>("Mirrors", "-"));
            _statusCard.Controls.Add(_chips);

            _startButton = MakeButton("Start relay", ButtonKind.Primary, Icons.Play);
            _startButton.Height = S(44);
            _startButton.Click += (s, e) => ToggleRelay();
            _statusCard.Controls.Add(_startButton);
        }

        private void BuildPreferencesCard()
        {
            _prefsCard = new Card();
            Controls.Add(_prefsCard);

            _prefsTitle = MakeLabel("Preferences", Theme.FontCardTitle, Theme.Text);
            _prefsCard.Controls.Add(_prefsTitle);

            _startupLabel = MakeLabel("Launch at Windows startup", Theme.FontBody, Theme.Text);
            _prefsCard.Controls.Add(_startupLabel);
            _startupSub = MakeLabel("Opens quietly in the tray", Theme.FontSmall, Theme.TextDim);
            _prefsCard.Controls.Add(_startupSub);

            _startupToggle = new ToggleSwitch { BackColor = Theme.Card, AccessibleName = "Launch at Windows startup" };
            _startupToggle.CheckedChanged += (s, e) => OnStartupToggled();
            _prefsCard.Controls.Add(_startupToggle);

            _autoStartLabel = MakeLabel("Start relay when the app opens", Theme.FontBody, Theme.Text);
            _prefsCard.Controls.Add(_autoStartLabel);
            _autoStartSub = MakeLabel("Keeps the addon online in the tray", Theme.FontSmall, Theme.TextDim);
            _prefsCard.Controls.Add(_autoStartSub);

            _autoStartToggle = new ToggleSwitch { BackColor = Theme.Card, AccessibleName = "Start relay when the app opens" };
            _autoStartToggle.CheckedChanged += (s, e) =>
            {
                _settings.AutoStartRelay = _autoStartToggle.Checked;
                _settings.Save();
            };
            _prefsCard.Controls.Add(_autoStartToggle);

            _prefsDivider = new Divider();
            _prefsCard.Controls.Add(_prefsDivider);

            _portLabel = MakeLabel("Relay port", Theme.FontBody, Theme.Text);
            _prefsCard.Controls.Add(_portLabel);
            _portSub = MakeLabel("Local addon server", Theme.FontSmall, Theme.TextDim);
            _prefsCard.Controls.Add(_portSub);

            _portPanel = new Card
            {
                FillColor = Theme.CardSoft,
                StrokeColor = Theme.Border,
                Radius = 8,
            };
            _prefsCard.Controls.Add(_portPanel);

            _portBox = new TextBox
            {
                BorderStyle = BorderStyle.None,
                BackColor = Theme.CardSoft,
                ForeColor = Theme.Text,
                Font = Theme.FontBodyStrong,
                TextAlign = HorizontalAlignment.Center,
                MaxLength = 5,
                Text = _settings.Port.ToString(),
                AccessibleName = "Relay port",
            };
            _portBox.KeyPress += (s, e) =>
            {
                if (!char.IsControl(e.KeyChar) && !char.IsDigit(e.KeyChar)) e.Handled = true;
            };
            _portBox.KeyDown += (s, e) =>
            {
                if (e.KeyCode == Keys.Enter)
                {
                    CommitPort();
                    e.SuppressKeyPress = true;
                }
            };
            _portBox.LostFocus += (s, e) => CommitPort();
            _portPanel.Controls.Add(_portBox);
            _tooltip.SetToolTip(_portBox, "Port for the local addon server (1024-65535).");

            _repairLink = MakeButton("Repair dependencies", ButtonKind.Link, Icons.Refresh);
            _repairLink.Font = Theme.FontSmall;
            _repairLink.Backplate = Theme.Card;
            _repairLink.Click += (s, e) => BeginSetup(false);
            _prefsCard.Controls.Add(_repairLink);

            _adminLink = MakeButton("Run as administrator", ButtonKind.Link, Icons.Shield);
            _adminLink.Font = Theme.FontSmall;
            _adminLink.Backplate = Theme.Card;
            _adminLink.Click += (s, e) => RelaunchElevated();
            _prefsCard.Controls.Add(_adminLink);
        }

        private void BuildInstallCard()
        {
            _installCard = new Card();
            Controls.Add(_installCard);

            _installTitle = MakeLabel("Stremio addon link", Theme.FontCardTitle, Theme.Text);
            _installCard.Controls.Add(_installTitle);

            _copyTopButton = new AppButton
            {
                Kind = ButtonKind.Quiet,
                IconPainter = Icons.Copy,
                Backplate = Theme.Card,
                AccessibleName = "Copy addon link",
                TabStop = false,
            };
            _copyTopButton.Click += (s, e) => CopyLink();
            _installCard.Controls.Add(_copyTopButton);
            _tooltip.SetToolTip(_copyTopButton, "Copy the install link");

            _urlPanel = new Card
            {
                FillColor = Theme.CardSoft,
                StrokeColor = Theme.Border,
                Radius = 10,
            };
            _installCard.Controls.Add(_urlPanel);

            _urlBox = new TextBox
            {
                BorderStyle = BorderStyle.None,
                BackColor = Theme.CardSoft,
                ForeColor = Theme.TextDim,
                Font = Theme.FontMono,
                ReadOnly = true,
                Text = LinkPlaceholder,
                TabStop = false,
                AccessibleName = "Stremio addon install link",
            };
            _urlPanel.Controls.Add(_urlBox);

            _installButton = MakeButton("Install in Stremio", ButtonKind.Secondary, null);
            _installButton.Enabled = false;
            _installButton.Click += (s, e) => OpenStremio();
            _installCard.Controls.Add(_installButton);

            _copyButton = MakeButton("Copy link", ButtonKind.Secondary, Icons.Copy);
            _copyButton.Enabled = false;
            _copyButton.Click += (s, e) => CopyLink();
            _installCard.Controls.Add(_copyButton);

            _openButton = MakeButton(string.Empty, ButtonKind.Ghost, Icons.External);
            _openButton.Enabled = false;
            _openButton.AccessibleName = "Open addon page in browser";
            _openButton.Click += (s, e) => OpenPage();
            _installCard.Controls.Add(_openButton);
            _tooltip.SetToolTip(_openButton, "Open the addon page");

            _urlHint = MakeLabel(
                "In Stremio: Addons, then Install from URL. Quick-tunnel links change when the relay restarts.",
                Theme.FontSmall, Theme.TextDim);
            _installCard.Controls.Add(_urlHint);
        }

        private void BuildLogCard()
        {
            _logsCard = new Card();
            Controls.Add(_logsCard);

            _logsTitle = MakeLabel("Live output", Theme.FontCardTitle, Theme.Text);
            _logsCard.Controls.Add(_logsTitle);

            _folderButton = new AppButton
            {
                Kind = ButtonKind.Quiet,
                IconPainter = Icons.Folder,
                Backplate = Theme.Card,
                AccessibleName = "Open log folder",
                TabStop = false,
            };
            _folderButton.Click += (s, e) => OpenLogFolder();
            _logsCard.Controls.Add(_folderButton);
            _tooltip.SetToolTip(_folderButton, "Open the log folder");

            _clearButton = MakeButton("Clear", ButtonKind.Quiet, null);
            _clearButton.Font = Theme.FontSmall;
            _clearButton.Click += (s, e) => ClearLogs();
            _logsCard.Controls.Add(_clearButton);

            _followLabel = MakeLabel("Follow", Theme.FontSmall, Theme.TextMuted);
            _logsCard.Controls.Add(_followLabel);

            _followToggle = new ToggleSwitch { BackColor = Theme.Card, AccessibleName = "Follow new log lines" };
            _followToggle.SetCheckedSilently(true);
            _followToggle.CheckedChanged += (s, e) =>
            {
                _settings.FollowLogs = _followToggle.Checked;
                _settings.Save();
            };
            _logsCard.Controls.Add(_followToggle);

            _filterButton = MakeButton("Info & up", ButtonKind.Secondary, null);
            _filterButton.Font = Theme.FontSmall;
            _filterButton.Click += (s, e) => CycleFilter();
            _logsCard.Controls.Add(_filterButton);

            _consolePanel = new Card
            {
                FillColor = Color.FromArgb(11, 11, 24),
                StrokeColor = Theme.BorderSoft,
                Radius = 10,
            };
            _logsCard.Controls.Add(_consolePanel);

            _logView = new RichTextBox
            {
                BorderStyle = BorderStyle.None,
                BackColor = Color.FromArgb(11, 11, 24),
                ForeColor = Theme.TextMuted,
                Font = Theme.FontMono,
                ReadOnly = true,
                DetectUrls = false,
                WordWrap = true,
                HideSelection = false,
                ScrollBars = RichTextBoxScrollBars.Vertical,
                ShortcutsEnabled = true,
                TabStop = false,
                AccessibleName = "Relay log output",
            };
            _logView.HandleCreated += (s, e) =>
            {
                try { SetWindowTheme(_logView.Handle, "DarkMode_Explorer", null); } catch { }
            };
            _consolePanel.Controls.Add(_logView);

            _logPath = MakeLabel("logs\\relay-desktop.log", Theme.FontTiny, Theme.TextDim);
            _logsCard.Controls.Add(_logPath);

            _logCountLabel = MakeLabel("0 lines", Theme.FontTiny, Theme.TextDim);
            _logCountLabel.TextAlign = ContentAlignment.MiddleRight;
            _logsCard.Controls.Add(_logCountLabel);
        }

        private void BuildTray()
        {
            _trayMenu = new ContextMenuStrip();
            _trayMenu.Items.Add("Open dashboard", null, (s, e) => ShowFromTray());
            _trayMenu.Items.Add(new ToolStripSeparator());
            _trayStart = new ToolStripMenuItem("Start relay", null, (s, e) => StartRelay());
            _trayMenu.Items.Add(_trayStart);
            _trayStop = new ToolStripMenuItem("Stop relay", null, (s, e) => StopRelay());
            _trayMenu.Items.Add(_trayStop);
            _trayCopy = new ToolStripMenuItem("Copy Stremio link", null, (s, e) => CopyLink());
            _trayMenu.Items.Add(_trayCopy);
            _trayMenu.Items.Add(new ToolStripSeparator());
            _trayMenu.Items.Add("Exit DhakaFlix Relay...", null, (s, e) => ExitApp());

            _tray = new NotifyIcon
            {
                Icon = ResolveTrayIcon(),
                Text = "DhakaFlix Relay",
                Visible = true,
                ContextMenuStrip = _trayMenu,
            };
            _tray.DoubleClick += (s, e) => ShowFromTray();
            _tray.BalloonTipClicked += (s, e) => ShowFromTray();
        }

        // ------------------------------------------------------------------ layout

        [System.Runtime.InteropServices.DllImport("dwmapi.dll")]
        private static extern int DwmSetWindowAttribute(IntPtr hwnd, int attribute, ref int value, int size);

        [System.Runtime.InteropServices.DllImport("uxtheme.dll", CharSet = System.Runtime.InteropServices.CharSet.Unicode)]
        private static extern int SetWindowTheme(IntPtr hwnd, string subApp, string subIdList);

        protected override void OnHandleCreated(EventArgs e)
        {
            base.OnHandleCreated(e);
            try
            {
                int dark = 1;
                if (DwmSetWindowAttribute(Handle, 20, ref dark, sizeof(int)) != 0)
                    DwmSetWindowAttribute(Handle, 19, ref dark, sizeof(int));
            }
            catch
            {
                // older Windows keeps the light title bar
            }
        }

        protected override void OnPaintBackground(PaintEventArgs e)
        {
            e.Graphics.Clear(Theme.Bg);
        }

        protected override void OnPaint(PaintEventArgs e)
        {
            base.OnPaint(e);
            using (var brush = new LinearGradientBrush(new Rectangle(0, 0, Math.Max(1, Width), S(3)),
                       Theme.Indigo, Theme.Accent, 0f))
            {
                e.Graphics.FillRectangle(brush, new Rectangle(0, 0, Width, S(3)));
            }
        }

        protected override void OnResize(EventArgs e)
        {
            base.OnResize(e);
            LayoutAll();
        }

        private void LayoutAll()
        {
            if (_brand == null) return;

            int width = ClientSize.Width;
            int height = ClientSize.Height;
            int pad = S(20);
            int headerH = S(42);
            int headerTop = S(18);

            _brand.SetBounds(pad, headerTop, S(40), S(40));
            _title.SetBounds(pad + S(52), headerTop - S(2), S(420), S(26));
            _subtitle.SetBounds(pad + S(52), headerTop + S(24), S(420), S(18));

            int pillW = S(126);
            int pillH = S(30);
            _statusPill.SetBounds(width - pad - pillW, headerTop + S(6), pillW, pillH);
            _version.SetBounds(width - pad - pillW - S(70), headerTop + S(6), S(62), pillH);
            _version.Visible = width > S(1000);

            int headerBottom = headerTop + headerH;
            int leftW = S(336);
            int gutter = S(16);

            int bannerH = S(76);
            int bannerTop = headerBottom + S(12);
            _banner.SetBounds(pad, bannerTop, width - pad * 2, bannerH);
            LayoutBanner(0, 0, width - pad * 2, bannerH);

            int mainTop = _banner.Visible ? bannerTop + bannerH + S(14) : headerBottom + S(14);
            int mainBottom = height - pad;
            if (mainBottom - mainTop < S(400))
            {
                mainTop = Math.Max(headerBottom + S(10), mainBottom - S(400));
            }

            int statusH = S(214);
            int prefsH = S(236);
            _statusCard.SetBounds(pad, mainTop, leftW, statusH);
            LayoutStatusCard(0, 0, leftW, statusH);

            int prefsTop = mainTop + statusH + S(14);
            int prefsHeight = Math.Min(prefsH, mainBottom - prefsTop);
            _prefsCard.SetBounds(pad, prefsTop, leftW, prefsHeight);
            LayoutPreferences(0, 0, leftW, prefsHeight);

            int rightX = pad + leftW + gutter;
            int rightW = width - pad - rightX;
            int installH = S(202);
            _installCard.SetBounds(rightX, mainTop, rightW, installH);
            LayoutInstallCard(0, 0, rightW, installH);

            int logsTop = mainTop + installH + S(14);
            int logsH = mainBottom - logsTop;
            _logsCard.SetBounds(rightX, logsTop, rightW, Math.Max(S(150), logsH));
            LayoutLogsCard(0, 0, rightW, Math.Max(S(150), logsH));
        }

        private void LayoutBanner(int x, int y, int w, int h)
        {
            int pad = S(14);
            _bannerTitle.SetBounds(x + S(54), y + S(12), w - S(54) - S(210), S(20));
            _bannerDetail.SetBounds(x + S(54), y + S(33), w - S(54) - S(220), S(17));
            _bannerPercent.SetBounds(x + w - S(230), y + S(35), S(46), S(17));
            _setupButton.SetBounds(x + w - pad - S(168), y + S(18), S(168), S(38));
            _setupButton.BringToFront();
            _progress.SetBounds(x + pad, y + h - S(9), w - pad * 2, S(5));
        }

        private void LayoutStatusCard(int x, int y, int w, int h)
        {
            int pad = S(16);
            _statusLabel.SetBounds(x + pad, y + S(15), w - pad * 2, S(14));
            _statusTitle.SetBounds(x + pad, y + S(32), w - pad * 2, S(28));
            _statusDetail.SetBounds(x + pad, y + S(63), w - pad * 2, S(34));
            _chips.SetBounds(x + pad, y + S(103), w - pad * 2, S(24));
            _startButton.SetBounds(x + pad, y + h - S(16) - S(44), w - pad * 2, S(44));
        }

        private void LayoutPreferences(int x, int y, int w, int h)
        {
            int pad = S(16);
            _prefsTitle.SetBounds(x + pad, y + S(14), w - pad * 2, S(18));

            int row1 = y + S(40);
            _startupLabel.SetBounds(x + pad, row1, w - pad * 2 - S(56), S(19));
            _startupSub.SetBounds(x + pad, row1 + S(19), w - pad * 2 - S(56), S(16));
            _startupToggle.SetBounds(x + w - pad - S(38), row1 + S(6), S(38), S(21));

            int row2 = y + S(82);
            _autoStartLabel.SetBounds(x + pad, row2, w - pad * 2 - S(56), S(19));
            _autoStartSub.SetBounds(x + pad, row2 + S(19), w - pad * 2 - S(56), S(16));
            _autoStartToggle.SetBounds(x + w - pad - S(38), row2 + S(6), S(38), S(21));

            _prefsDivider.SetBounds(x + pad, y + S(126), w - pad * 2, 1);

            _portLabel.SetBounds(x + pad, y + S(139), S(140), S(19));
            _portSub.SetBounds(x + pad, y + S(158), S(160), S(16));
            _portPanel.SetBounds(x + w - pad - S(74), y + S(140), S(74), S(30));
            _portBox.SetBounds(S(8), S(6), S(58), S(18));

            _repairLink.SetBounds(x + pad, h + y - S(32), S(150), S(22));
            _adminLink.SetBounds(x + pad + S(154), h + y - S(32), S(180), S(22));
        }

        private void LayoutInstallCard(int x, int y, int w, int h)
        {
            int pad = S(16);
            int innerW = w - pad * 2;

            _installTitle.SetBounds(x + pad, y + S(14), innerW - S(40), S(18));
            _copyTopButton.SetBounds(x + w - pad - S(30), y + S(10), S(30), S(30));

            _urlPanel.SetBounds(x + pad, y + S(42), innerW, S(44));
            _urlBox.SetBounds(S(9), S(11), innerW - S(18), S(20));

            int buttonTop = y + S(98);
            int openW = S(42);
            int copyW = S(96);
            int installW = innerW - openW - copyW - S(16);
            _installButton.SetBounds(x + pad, buttonTop, installW, S(42));
            _copyButton.SetBounds(x + pad + installW + S(8), buttonTop, copyW, S(42));
            _openButton.SetBounds(x + pad + installW + copyW + S(16), buttonTop, openW, S(42));

            _urlHint.SetBounds(x + pad, y + S(150), innerW, S(36));
        }

        private void LayoutLogsCard(int x, int y, int w, int h)
        {
            int pad = S(14);
            _logsTitle.SetBounds(x + pad, y + S(11), S(150), S(20));

            int toolTop = y + S(8);
            int toolH = S(28);
            int right = x + w - pad;

            _folderButton.SetBounds(right - S(30), toolTop, S(30), toolH);
            _clearButton.SetBounds(right - S(30) - S(8) - S(50), toolTop, S(50), toolH);
            _followToggle.SetBounds(right - S(30) - S(8) - S(50) - S(10) - S(38), toolTop + S(4), S(38), S(21));
            _followLabel.SetBounds(right - S(30) - S(8) - S(50) - S(10) - S(38) - S(48), toolTop + S(4), S(44), S(20));
            _followLabel.TextAlign = ContentAlignment.MiddleRight;
            _filterButton.SetBounds(right - S(30) - S(8) - S(50) - S(10) - S(38) - S(48) - S(8) - S(94), toolTop, S(94), toolH);

            int consoleTop = y + S(44);
            int consoleH = h - S(44) - S(30);
            _consolePanel.SetBounds(x + pad, consoleTop, w - pad * 2, Math.Max(S(60), consoleH));
            _logView.SetBounds(S(10), S(8), _consolePanel.Width - S(20), _consolePanel.Height - S(16));

            _logPath.SetBounds(x + pad, y + h - S(23), w - pad * 2 - S(90), S(16));
            _logCountLabel.SetBounds(x + w - pad - S(90), y + h - S(23), S(90), S(16));
        }

        // ----------------------------------------------------------------- lifecycle

        private void OnLoaded()
        {
            _suppressToggle = true;
            _startupToggle.SetCheckedSilently(StartupManager.IsEnabled());
            _autoStartToggle.SetCheckedSilently(_settings.AutoStartRelay);
            _followToggle.SetCheckedSilently(_settings.FollowLogs);
            _suppressToggle = false;

            _adminLink.Visible = !Elevation.IsAdministrator();
            if (!_adminLink.Visible) _repairLink.SetBounds(_adminLink.Left, _adminLink.Top, S(160), S(22));

            _urlBox.Text = LinkPlaceholder;
            _urlHint.Text = "In Stremio: Addons, then Install from URL. Quick-tunnel links change when the relay restarts.";
            OpenLogFile();
            AppendLocalLog(LogLevel.Info, "Desktop started. Relay folder: " + _paths.Root);

            RefreshDependencies();
        }

        private void OnFirstShown()
        {
            if (_autostart)
            {
                Hide();
                if (_settings.AutoStartRelay)
                {
                    _autoStartPending = true;
                    StartWhenReady();
                }
                return;
            }
            Activate();
        }

        private async void RefreshDependencies()
        {
            try
            {
                DependencySnapshot snapshot = await Task.Run(() => _deps.Inspect());
                _snapshot = snapshot;
                ApplySetupState();
            }
            catch (Exception ex)
            {
                AppendLocalLog(LogLevel.Error, "Dependency check failed: " + ex.Message);
            }
        }

        private void ApplySetupState()
        {
            if (_snapshot == null) return;

            if (_snapshot.Ready)
            {
                _banner.Visible = false;
                _progress.Visible = false;
                _progress.Indeterminate = false;
                _statusTitle.ForeColor = Theme.Text;
                UpdateStartButton();
                LayoutAll();
                if (_autoStartPending && !_relay.IsActive)
                {
                    _autoStartPending = false;
                    StartRelay();
                }
                return;
            }

            _banner.Visible = true;
            SetBannerPalette(false);
            _bannerTitle.Text = _installing ? "Setting up the relay" : "Finish setup to start the relay";
            _bannerDetail.Text = _installing
                ? "Working..."
                : "Missing: " + string.Join(", ", _snapshot.Missing().ToArray()) + ". Installed locally, no admin needed.";
            _bannerPercent.Text = string.Empty;
            _setupButton.Text = _installing ? "Installing..." : "Install dependencies";
            _setupButton.Enabled = !_installing;
            _progress.Visible = _installing;
            _statusTitle.ForeColor = Theme.Text;
            LayoutAll();
        }

        private void UpdateStartButton()
        {
            bool ready = _snapshot != null && _snapshot.Ready;
            switch (_relay.State)
            {
                case RelayState.Starting:
                    _startButton.Text = "Starting...";
                    _startButton.Kind = ButtonKind.Secondary;
                    _startButton.IconPainter = null;
                    _startButton.Enabled = false;
                    break;
                case RelayState.Running:
                    _startButton.Text = "Stop relay";
                    _startButton.Kind = ButtonKind.Danger;
                    _startButton.IconPainter = Icons.Stop;
                    _startButton.Enabled = true;
                    break;
                case RelayState.Stopping:
                    _startButton.Text = "Stopping...";
                    _startButton.Kind = ButtonKind.Secondary;
                    _startButton.IconPainter = null;
                    _startButton.Enabled = false;
                    break;
                case RelayState.Error:
                    _startButton.Text = ready ? "Start relay" : "Finish setup";
                    _startButton.Kind = ButtonKind.Primary;
                    _startButton.IconPainter = ready ? (IconPainter)Icons.Play : null;
                    _startButton.Enabled = !_installing;
                    break;
                default:
                    _startButton.Text = ready ? "Start relay" : "Finish setup";
                    _startButton.Kind = ButtonKind.Primary;
                    _startButton.IconPainter = ready ? (IconPainter)Icons.Play : Icons.Shield;
                    _startButton.Enabled = !_installing;
                    break;
            }
            _startButton.Invalidate();
        }

        // ----------------------------------------------------------------- relay

        private void ToggleRelay()
        {
            if (_relay.IsActive)
            {
                StopRelay();
                return;
            }

            if (_snapshot == null || !_snapshot.Ready)
            {
                _setupThenStart = true;
                BeginSetup(true);
                return;
            }

            StartRelay();
        }

        private async void StartRelay()
        {
            if (_relay.IsActive) return;
            if (_snapshot == null || !_snapshot.Ready)
            {
                _setupThenStart = true;
                BeginSetup(true);
                return;
            }

            try
            {
                await _relay.StartAsync();
                _healthTimer.Start();
            }
            catch (Exception ex)
            {
                AppendLocalLog(LogLevel.Error, ex.Message);
                _statusDetail.Text = ex.Message;
            }
        }

        private async void StopRelay()
        {
            _healthTimer.Stop();
            try
            {
                await _relay.StopAsync();
            }
            catch (Exception ex)
            {
                AppendLocalLog(LogLevel.Warn, "Stop reported: " + ex.Message);
            }
        }

        private async void StartWhenReady()
        {
            if (_snapshot == null)
            {
                await Task.Delay(600);
            }
            if (_snapshot != null && _snapshot.Ready && _settings.AutoStartRelay && !_relay.IsActive)
            {
                _autoStartPending = false;
                StartRelay();
            }
        }

        private void ApplyRelayState(RelayState state, string detail)
        {
            switch (state)
            {
                case RelayState.Starting:
                    _statusPill.Set("Starting", Theme.WarningSoft, true);
                    _statusTitle.Text = "Starting";
                    _statusDetail.Text = detail ?? "Creating the public tunnel...";
                    _statusDetail.ForeColor = Theme.TextMuted;
                    ClearLink();
                    _healthTimer.Stop();
                    break;

                case RelayState.Running:
                    _statusPill.Set("Running", Theme.Accent, true);
                    _statusTitle.Text = "Running";
                    _statusDetail.Text = detail ?? "The addon is live.";
                    _statusDetail.ForeColor = Theme.TextMuted;
                    _healthTimer.Start();
                    break;

                case RelayState.Stopping:
                    _statusPill.Set("Stopping", Theme.IndigoSoft, true);
                    _statusTitle.Text = "Stopping";
                    _statusDetail.Text = "Closing the relay and tunnel...";
                    _statusDetail.ForeColor = Theme.TextMuted;
                    _healthTimer.Stop();
                    break;

                case RelayState.Error:
                    _statusPill.Set("Error", Theme.Danger, false);
                    _statusTitle.Text = "Needs attention";
                    _statusDetail.Text = detail ?? "The relay stopped unexpectedly.";
                    _statusDetail.ForeColor = Theme.Danger;
                    _healthTimer.Stop();
                    ClearLink();
                    break;

                default:
                    _statusPill.Set("Stopped", Theme.TextDim, false);
                    _statusTitle.Text = "Stopped";
                    _statusDetail.Text = "Start the relay to get your Stremio install link.";
                    _statusDetail.ForeColor = Theme.TextMuted;
                    _healthTimer.Stop();
                    ClearLink();
                    break;
            }

            _portBox.Enabled = !_relay.IsActive;
            _portPanel.FillColor = _portBox.Enabled ? Theme.CardSoft : Theme.BgSoft;
            _portBox.BackColor = _portPanel.FillColor;
            _portPanel.Invalidate();
            _portSub.Text = _relay.IsActive ? "Restart relay to change" : "Local addon server";

            UpdateStartButton();
            UpdateChips();
            UpdateTray();
            LayoutAll();
        }

        private void UpdateChips()
        {
            string uptime = "--:--";
            if (_relay.IsActive && _relay.State != RelayState.Stopping)
            {
                TimeSpan span = _relay.Uptime;
                uptime = string.Format("{0:00}:{1:00}:{2:00}", (int)span.TotalHours, span.Minutes, span.Seconds);
            }
            string mirrors = "-";
            if (_mirrorsKnown > 0) mirrors = _mirrorsOnline + "/" + _mirrorsKnown;
            _chips.SetItems(
                new KeyValuePair<string, string>("Port", _settings.Port.ToString()),
                new KeyValuePair<string, string>("Uptime", uptime),
                new KeyValuePair<string, string>("Mirrors", mirrors));
            _chips.AccessibleName = _chips.Describe();
        }

        private int _mirrorsOnline;
        private int _mirrorsKnown;

        private void ApplyHealth(HealthSnapshot health)
        {
            if (health == null) return;
            _mirrorsOnline = health.ProvidersOnline;
            _mirrorsKnown = health.ProvidersKnown;
            UpdateChips();
        }

        private async void PollHealth()
        {
            try
            {
                HealthSnapshot health = await _relay.FetchHealthAsync();
                if (health != null) ApplyHealth(health);
            }
            catch
            {
                // best effort
            }
        }

        // ----------------------------------------------------------------- links

        private void ApplyManifest(string manifest)
        {
            if (string.IsNullOrEmpty(manifest)) return;
            _hasLink = true;
            _lastManifest = manifest;
            _lastIssuedLink = manifest;
            _urlBox.Text = manifest;
            _urlBox.ForeColor = Theme.Text;
            _urlBox.SelectionStart = 0;
            _urlBox.SelectionLength = 0;
            _installButton.Enabled = true;
            _installButton.Kind = ButtonKind.Primary;
            _installButton.Invalidate();
            _copyButton.Enabled = true;
            _openButton.Enabled = true;
            _urlHint.Text = manifest.IndexOf("trycloudflare.com", StringComparison.OrdinalIgnoreCase) >= 0
                ? "Quick tunnel link - it changes every time the relay restarts. Use a named tunnel for a permanent URL."
                : "Local link - works in Stremio on this PC. Public tunnel was unavailable.";
            RefreshTrayState();
            MaybePromoteFirstInstall();
        }

        private void ClearLink()
        {
            _hasLink = false;
            _lastManifest = null;
            _urlBox.Text = LinkPlaceholder;
            _urlBox.ForeColor = Theme.TextDim;
            _installButton.Enabled = false;
            _installButton.Kind = ButtonKind.Secondary;
            _installButton.Invalidate();
            _copyButton.Enabled = false;
            _openButton.Enabled = false;
            _urlHint.Text = "In Stremio: Addons, then Install from URL. Quick-tunnel links change when the relay restarts.";
            RefreshTrayState();
        }

        private void CopyLink()
        {
            if (!_hasLink || string.IsNullOrEmpty(_lastManifest)) return;
            try
            {
                Clipboard.SetText(_lastManifest);
                _copyButton.Text = "Copied";
                _copyTopButton.Text = string.Empty;
                _copyTimer.Stop();
                _copyTimer.Start();
            }
            catch
            {
                AppendLocalLog(LogLevel.Warn, "Clipboard is busy; select the link and copy it manually.");
            }
        }

        private void OpenStremio()
        {
            if (!_hasLink) return;

            string error;
            if (StremioHelper.OpenInstall(_lastManifest, out error))
            {
                AppendLocalLog(LogLevel.Info, "Opened Stremio install link");
                return;
            }

            AppendLocalLog(LogLevel.Error, "Could not open Stremio: " + error);
            ShowStremioMissing();
        }

        private async void MaybePromoteFirstInstall()
        {
            if (!_settings.FirstRun || _firstRunPromoted || _autostart) return;
            _firstRunPromoted = true;
            _settings.FirstRun = false;
            _settings.Save();

            await Task.Delay(700);
            if (IsDisposed || !_hasLink || string.IsNullOrEmpty(_lastManifest)) return;

            if (!StremioHelper.IsInstalled())
            {
                ShowStremioMissing();
                return;
            }

            AppendLocalLog(LogLevel.Info, "First run - opening Stremio so the addon can be installed");
            string error;
            if (!StremioHelper.OpenInstall(_lastManifest, out error))
            {
                AppendLocalLog(LogLevel.Warn, "Could not open Stremio: " + error);
                ShowStremioMissing();
            }
        }

        private void ShowStremioMissing()
        {
            DialogResult answer = MessageBox.Show(this,
                "Stremio is not installed on this PC." + Environment.NewLine + Environment.NewLine +
                "Install Stremio, then click \"Install in Stremio\" in this window to add the addon." + Environment.NewLine + Environment.NewLine +
                "Open the Stremio download page now?",
                "Stremio not found", MessageBoxButtons.YesNo, MessageBoxIcon.Information);
            if (answer == DialogResult.Yes) StremioHelper.OpenDownloadPage();
        }

        private void OpenPage()
        {
            try
            {
                string url = _relay.PublicUrl ?? _relay.LocalUrl;
                Process.Start(new ProcessStartInfo(url) { UseShellExecute = true });
            }
            catch (Exception ex)
            {
                AppendLocalLog(LogLevel.Warn, "Could not open the browser: " + ex.Message);
            }
        }

        private void OpenLogFolder()
        {
            try
            {
                _paths.EnsureFolders();
                Process.Start("explorer.exe", "\"" + _paths.LogsDir + "\"");
            }
            catch
            {
                // explorer unavailable
            }
        }

        // ----------------------------------------------------------------- setup

        private async void BeginSetup(bool startAfter)
        {
            if (_installing) return;
            if (_relay.IsActive)
            {
                await _relay.StopAsync();
            }

            _installing = true;
            _setupThenStart = startAfter;
            _banner.Visible = true;
            SetBannerPalette(false);
            _bannerTitle.Text = "Setting up the relay";
            _bannerDetail.Text = "Preparing...";
            _bannerPercent.Text = "0%";
            _setupButton.Text = "Installing...";
            _setupButton.Enabled = false;
            _progress.Visible = true;
            _progress.Value = 0;
            _progress.Indeterminate = false;
            LayoutAll();
            UpdateStartButton();

            var progress = new Progress<SetupProgress>(OnSetupProgress);
            bool ok = await _deps.InstallAsync(progress, CancellationToken.None);

            _installing = false;
            await RefreshDependenciesAsync();

            if (ok)
            {
                _progress.Indeterminate = false;
                _progress.Value = 100;
                _bannerTitle.Text = "Setup complete";
                _bannerDetail.Text = "Everything is installed. Starting the relay...";
                _bannerPercent.Text = "100%";
                _banner.Visible = false;
                AppendLocalLog(LogLevel.Info, "Setup finished");
                UpdateStartButton();
                if (_setupThenStart) StartRelay();
            }
            else
            {
                _progress.Indeterminate = false;
                _progress.Value = 0;
                _progress.Visible = false;
                SetBannerPalette(true);
                _bannerTitle.Text = "Setup did not finish";
                _bannerDetail.Text = "See the live output for the exact error, then retry.";
                _bannerPercent.Text = string.Empty;
                _setupButton.Text = "Retry setup";
                _setupButton.Enabled = true;
                LayoutAll();

                if (_snapshot == null || !_snapshot.NodeOk)
                {
                    DialogResult answer = MessageBox.Show(this,
                        "Node.js could not be prepared automatically.\r\n\r\n" +
                        "Install Node.js 18+ now with administrator rights (UAC prompt)? " +
                        "This uses winget and is the only step that needs admin.",
                        "Node.js is required",
                        MessageBoxButtons.YesNo, MessageBoxIcon.Question);
                    if (answer == DialogResult.Yes)
                    {
                        bool installed = await Task.Run(() => _deps.TryInstallNodeSystemWide());
                        await RefreshDependenciesAsync();
                        if (installed && _snapshot != null && _snapshot.NodeOk)
                        {
                            BeginSetup(_setupThenStart);
                        }
                    }
                }
            }
        }

        private void OnSetupProgress(SetupProgress progress)
        {
            if (progress == null) return;
            _progress.Visible = true;
            _progress.Indeterminate = progress.Percent <= 0 || string.Equals(progress.Step, "Installing cloudflared", StringComparison.OrdinalIgnoreCase) || string.Equals(progress.Step, "Installing npm packages", StringComparison.OrdinalIgnoreCase);
            if (!_progress.Indeterminate) _progress.Value = progress.Percent;
            _bannerTitle.Text = "Setting up the relay";
            _bannerDetail.Text = string.IsNullOrEmpty(progress.Detail)
                ? progress.Step
                : progress.Step + " - " + progress.Detail;
            _bannerPercent.Text = progress.Percent > 0 ? progress.Percent + "%" : string.Empty;
        }

        private async Task RefreshDependenciesAsync()
        {
            try
            {
                _snapshot = await Task.Run(() => _deps.Inspect());
                ApplySetupState();
            }
            catch (Exception ex)
            {
                AppendLocalLog(LogLevel.Error, "Dependency check failed: " + ex.Message);
            }
        }

        // ----------------------------------------------------------------- prefs

        private void OnStartupToggled()
        {
            if (_suppressToggle) return;
            bool enabled = _startupToggle.Checked;
            bool ok = enabled ? StartupManager.Enable(Application.ExecutablePath) : StartupManager.Disable();
            if (!ok)
            {
                _suppressToggle = true;
                _startupToggle.SetCheckedSilently(!enabled);
                _suppressToggle = false;
                MessageBox.Show(this, "Windows did not accept the startup change. Try again from a normal user account.",
                    "Startup setting", MessageBoxButtons.OK, MessageBoxIcon.Warning);
                return;
            }
            AppendLocalLog(LogLevel.Info, enabled
                ? "Relay will start with Windows (per-user Run key)"
                : "Startup entry removed");
        }

        private void CommitPort()
        {
            if (_relay.IsActive) return;
            int port;
            if (!int.TryParse(_portBox.Text.Trim(), out port) || port < 1024 || port > 65535)
            {
                _portBox.Text = _settings.Port.ToString();
                MessageBox.Show(this, "Choose a port between 1024 and 65535.", "Invalid port",
                    MessageBoxButtons.OK, MessageBoxIcon.Warning);
                return;
            }
            if (port == _settings.Port) return;
            _settings.Port = port;
            _settings.Save();
            _relay.UpdateLocalUrl(port);
            UpdateChips();
            AppendLocalLog(LogLevel.Info, "Relay port set to " + port);
        }

        private async void RelaunchElevated()
        {
            DialogResult answer = MessageBox.Show(this,
                "Restart DhakaFlix Relay with administrator rights?\r\n\r\n" +
                "The relay itself does not need admin. This is only useful for system-wide installs.",
                "Run as administrator", MessageBoxButtons.YesNo, MessageBoxIcon.Question);
            if (answer != DialogResult.Yes) return;

            if (_relay.IsActive) await _relay.StopAsync();

            string error;
            bool started = Elevation.StartElevated(Application.ExecutablePath,
                "--wait-pid=" + Process.GetCurrentProcess().Id, false, out error);
            if (!started)
            {
                MessageBox.Show(this, "Windows did not allow elevation: " + error, "Run as administrator",
                    MessageBoxButtons.OK, MessageBoxIcon.Warning);
                return;
            }

            _forceExit = true;
            Close();
        }

        // ----------------------------------------------------------------- logs

        private void OnRelayLog(LogEntry entry)
        {
            _pending.Enqueue(entry);
        }

        private void Post(Action action)
        {
            if (action == null || IsDisposed) return;
            try
            {
                if (InvokeRequired) BeginInvoke(action);
                else action();
            }
            catch (InvalidOperationException)
            {
                // window is closing
            }
        }

        private void PostLog(LogLevel level, string text)
        {
            _pending.Enqueue(new LogEntry(level, text));
        }

        private void AppendLocalLog(LogLevel level, string text)
        {
            _pending.Enqueue(new LogEntry(level, text));
        }

        private void DrainLogs()
        {
            bool appended = false;
            for (int i = 0; i < 160; i++)
            {
                LogEntry entry;
                if (!_pending.TryDequeue(out entry)) break;
                _logEntries.Add(entry);
                _logCount++;
                WriteLogFile(entry);
                if (PassesFilter(entry.Level))
                {
                    AppendToView(entry);
                    appended = true;
                }
            }

            if (_logEntries.Count > 4000)
            {
                _logEntries.RemoveRange(0, _logEntries.Count - 2500);
                RebuildView();
            }

            _logCountLabel.Text = _logCount + (_logCount == 1 ? " line" : " lines");

            if (appended && _settings.FollowLogs && _followToggle.Checked)
            {
                _logView.SelectionStart = _logView.TextLength;
                _logView.SelectionLength = 0;
                _logView.ScrollToCaret();
            }
        }

        private void OpenLogFile()
        {
            try
            {
                _paths.EnsureFolders();
                try
                {
                    var info = new FileInfo(_paths.LogFile);
                    if (info.Exists && info.Length > 5L * 1024L * 1024L)
                    {
                        string old = _paths.LogFile + ".old";
                        if (File.Exists(old)) File.Delete(old);
                        File.Move(_paths.LogFile, old);
                    }
                }
                catch
                {
                    // rotation is best-effort
                }
                var stream = new FileStream(_paths.LogFile, FileMode.Append, FileAccess.Write, FileShare.Read);
                _logWriter = new StreamWriter(stream, new UTF8Encoding(false)) { AutoFlush = true };
                _logWriter.WriteLine();
                _logWriter.WriteLine("===== " + DateTime.Now.ToString("yyyy-MM-dd HH:mm:ss") + " desktop session started =====");
            }
            catch
            {
                _logWriter = null;
            }
        }

        private void WriteLogFile(LogEntry entry)
        {
            if (_logWriter == null) return;
            lock (_logFileLock)
            {
                try
                {
                    _logWriter.WriteLine(FormatLine(entry));
                }
                catch
                {
                    // the live view keeps working even if the file cannot be written
                }
            }
        }

        private bool PassesFilter(LogLevel level)
        {
            switch (_filterMode)
            {
                case 0: return true;
                case 1: return level >= LogLevel.Info;
                case 2: return level >= LogLevel.Warn;
                case 3: return level >= LogLevel.Error;
                default: return true;
            }
        }

        private void AppendToView(LogEntry entry)
        {
            _logView.SelectionStart = _logView.TextLength;
            _logView.SelectionLength = 0;
            _logView.SelectionColor = ColorFor(entry.Level);
            _logView.AppendText(FormatLine(entry) + Environment.NewLine);
        }

        private static Color ColorFor(LogLevel level)
        {
            switch (level)
            {
                case LogLevel.Error: return Color.FromArgb(248, 113, 113);
                case LogLevel.Warn: return Color.FromArgb(251, 191, 36);
                case LogLevel.Debug: return Color.FromArgb(122, 137, 158);
                default: return Color.FromArgb(203, 213, 225);
            }
        }

        private static string FormatLine(LogEntry entry)
        {
            string stamp = entry.Time.ToString("HH:mm:ss");
            Match match = NodeLogLine.Match(entry.Text);
            if (match.Success)
            {
                string level = match.Groups[2].Value.ToUpperInvariant();
                string section = match.Groups[3].Value;
                string message = match.Groups[4].Value;
                return stamp + "  " + level.PadRight(5) + " [" + section + "] " + message;
            }
            return stamp + "  " + TagFor(entry.Level).PadRight(5) + " " + entry.Text;
        }

        private static string TagFor(LogLevel level)
        {
            switch (level)
            {
                case LogLevel.Error: return "ERROR";
                case LogLevel.Warn: return "WARN";
                case LogLevel.Debug: return "DEBUG";
                default: return "INFO";
            }
        }

        private void RebuildView()
        {
            _logView.Clear();
            foreach (LogEntry entry in _logEntries)
            {
                if (PassesFilter(entry.Level)) AppendToView(entry);
            }
        }

        private void CycleFilter()
        {
            _filterMode = (_filterMode + 1) % 4;
            switch (_filterMode)
            {
                case 0: _filterButton.Text = "All output"; break;
                case 1: _filterButton.Text = "Info & up"; break;
                case 2: _filterButton.Text = "Warnings"; break;
                default: _filterButton.Text = "Errors"; break;
            }
            RebuildView();
        }

        private void ClearLogs()
        {
            _logEntries.Clear();
            _logCount = 0;
            _logView.Clear();
            _logCountLabel.Text = "0 lines";
            AppendLocalLog(LogLevel.Info, "Log view cleared");
        }

        // ----------------------------------------------------------------- tray

        private Icon ResolveTrayIcon()
        {
            if (Icon != null) return Icon;

            using (var bitmap = new Bitmap(32, 32))
            {
                using (Graphics g = Graphics.FromImage(bitmap))
                {
                    g.SmoothingMode = SmoothingMode.AntiAlias;
                    g.Clear(Color.Transparent);
                    Theme.FillRoundedGradient(g, new RectangleF(0, 0, 32, 32), 8f,
                        Color.FromArgb(79, 70, 229), Color.FromArgb(124, 58, 237), 45f);
                    Icons.Play(g, new RectangleF(10, 8, 14, 16), Color.White);
                }
                return Icon.FromHandle(bitmap.GetHicon());
            }
        }

        private void UpdateTray()
        {
            if (_tray == null) return;
            string text;
            switch (_relay.State)
            {
                case RelayState.Running: text = "DhakaFlix Relay - running"; break;
                case RelayState.Starting: text = "DhakaFlix Relay - starting"; break;
                case RelayState.Error: text = "DhakaFlix Relay - needs attention"; break;
                case RelayState.Stopping: text = "DhakaFlix Relay - stopping"; break;
                default: text = "DhakaFlix Relay - stopped"; break;
            }
            _tray.Text = text.Length > 63 ? text.Substring(0, 63) : text;
            RefreshTrayState();
        }

        private void RefreshTrayState()
        {
            if (_trayMenu == null) return;
            _trayStart.Enabled = !_relay.IsActive && _snapshot != null && _snapshot.Ready;
            _trayStop.Enabled = _relay.IsActive;
            _trayCopy.Enabled = _hasLink;
        }

        private void ShowFromTray()
        {
            Show();
            if (WindowState == FormWindowState.Minimized) WindowState = FormWindowState.Normal;
            Activate();
            BringToFront();
        }

        public void SignalShow()
        {
            if (IsDisposed) return;
            if (InvokeRequired)
            {
                BeginInvoke(new Action(SignalShow));
                return;
            }
            ShowFromTray();
        }

        private void HideToTray()
        {
            Hide();
            if (!_trayHintShown && _tray != null)
            {
                _trayHintShown = true;
                try
                {
                    _tray.ShowBalloonTip(4500, "DhakaFlix Relay is still running",
                        "The relay stays online in the notification area. Right-click the tray icon and choose Exit to stop it.",
                        ToolTipIcon.Info);
                }
                catch
                {
                    // balloons can be disabled by policy
                }
            }
        }

        private bool ConfirmQuit()
        {
            using (var dialog = new QuitDialog(_lastIssuedLink ?? _lastManifest))
            {
                DialogResult result = dialog.ShowDialog(this);
                if (result == DialogResult.Yes) return true;
                if (dialog.OpenStremioRequested) OpenStremioForRemoval();
                return false;
            }
        }

        private void OpenStremioForRemoval()
        {
            string error;
            if (StremioHelper.OpenApp(out error))
            {
                AppendLocalLog(LogLevel.Info, "Opened Stremio so the addon can be removed");
                return;
            }

            AppendLocalLog(LogLevel.Warn, "Could not open Stremio: " + error);
            DialogResult answer = MessageBox.Show(this,
                "Stremio could not be opened." + Environment.NewLine + Environment.NewLine +
                "Open the Stremio download page instead?",
                "Stremio", MessageBoxButtons.YesNo, MessageBoxIcon.Information);
            if (answer == DialogResult.Yes) StremioHelper.OpenDownloadPage();
        }

        private async void ExitApp()
        {
            if (_quitting) return;

            if (!string.IsNullOrEmpty(_lastIssuedLink) && !ConfirmQuit())
                return;

            _quitting = true;
            _healthTimer.Stop();

            if (_relay.IsActive)
            {
                AppendLocalLog(LogLevel.Info, "Stopping relay before exit");
                try
                {
                    await _relay.StopAsync();
                }
                catch (Exception ex)
                {
                    AppendLocalLog(LogLevel.Warn, "Stop reported: " + ex.Message);
                }
            }

            _forceExit = true;
            if (_tray != null) _tray.Visible = false;
            Close();
        }

        private void OnFormClosing(object sender, FormClosingEventArgs e)
        {
            if (_forceExit) return;

            if (e.CloseReason == CloseReason.WindowsShutDown || e.CloseReason == CloseReason.TaskManagerClosing)
            {
                _forceExit = true;
                return;
            }

            // Closing the window never stops the relay: the tray icon owns the lifecycle.
            e.Cancel = true;
            HideToTray();
        }

        protected override void Dispose(bool disposing)
        {
            if (disposing)
            {
                _logTimer.Dispose();
                _uptimeTimer.Dispose();
                _healthTimer.Dispose();
                _copyTimer.Dispose();
                _tooltip.Dispose();
                if (_tray != null) _tray.Dispose();
                _relay.Dispose();
                lock (_logFileLock)
                {
                    if (_logWriter != null)
                    {
                        try { _logWriter.Dispose(); } catch { }
                        _logWriter = null;
                    }
                }
            }
            base.Dispose(disposing);
        }
    }
}
