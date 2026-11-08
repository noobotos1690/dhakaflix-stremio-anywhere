using System;
using System.Drawing;
using System.Threading;
using System.Threading.Tasks;
using System.Windows.Forms;

namespace DhakaFlixRelay
{
    /// <summary>
    /// Public access settings: Cloudflare Quick Tunnel or a stable Tailscale
    /// Funnel URL. All Tailscale work happens asynchronously.
    /// </summary>
    internal sealed class PublicAccessDialog : Form
    {
        private readonly MainForm _owner;
        private readonly AppSettings _settings;
        private readonly TailscaleManager _tailscale;

        private Card _cloudflareCard;
        private Card _tailscaleCard;
        private RadioDot _cloudflareDot;
        private Label _cloudflareTitle;
        private Label _cloudflareSub;
        private Label _tailscaleTitle;
        private Label _tailscaleSub;
        private RadioDot _tailscaleDot;
        private StatusPill _statusPill;
        private Label _statusDetail;
        private Label _hostValue;
        private Label _funnelValue;
        private Label _message;
        private Label _publicValue;
        private Label _manifestValue;
        private AppButton _installButton;
        private AppButton _connectButton;
        private AppButton _refreshButton;
        private AppButton _enableButton;
        private AppButton _disableButton;
        private AppButton _copyButton;
        private AppButton _stremioButton;
        private AppButton _closeButton;

        private TailscaleStatusInfo _status;
        private TailscaleFunnelInfo _funnel;
        private bool _busy;
        private bool _refreshPending;
        private string _publicUrl = string.Empty;
        private string _manifestUrl = string.Empty;

        public PublicAccessDialog(MainForm owner)
        {
            _owner = owner;
            _settings = owner.Settings;
            _tailscale = owner.Tailscale;

            Text = "Public access - DhakaFlix Relay";
            BackColor = Theme.Bg;
            ForeColor = Theme.Text;
            Font = Theme.FontBody;
            FormBorderStyle = FormBorderStyle.FixedDialog;
            MaximizeBox = false;
            MinimizeBox = false;
            ShowInTaskbar = false;
            StartPosition = FormStartPosition.CenterParent;
            ClientSize = new Size(660, 720);
            DoubleBuffered = true;
            AutoScaleMode = AutoScaleMode.Dpi;
            AutoScaleDimensions = new SizeF(96F, 96F);
            try { Icon = Icon.ExtractAssociatedIcon(Application.ExecutablePath); } catch { }

            var card = new Card { FillColor = Theme.Card, Radius = 14 };
            card.SetBounds(20, 20, 620, 680);
            Controls.Add(card);

            Label title = MakeLabel("Public access", 14f, FontStyle.Bold, Theme.Text);
            title.SetBounds(18, 12, 584, 28);
            card.Controls.Add(title);

            Label subtitle = MakeLabel(
                "Choose how Stremio reaches the relay. Both modes keep the same streaming, seeking and resume behaviour.",
                8.5f, FontStyle.Regular, Theme.TextMuted);
            subtitle.SetBounds(18, 40, 584, 20);
            card.Controls.Add(subtitle);

            _cloudflareCard = MakeModeCard(card, 18, 72, "Cloudflare Quick Tunnel",
                "Temporary hostname, new on every start", PublicAccessMode.Cloudflare,
                out _cloudflareDot, out _cloudflareTitle, out _cloudflareSub);
            _tailscaleCard = MakeModeCard(card, 314, 72, "Tailscale Funnel",
                "Permanent HTTPS URL behind CGNAT", PublicAccessMode.Tailscale,
                out _tailscaleDot, out _tailscaleTitle, out _tailscaleSub);

            var statusCard = new Card
            {
                FillColor = Theme.CardSoft,
                StrokeColor = Theme.BorderSoft,
                Radius = 10,
            };
            statusCard.SetBounds(18, 160, 584, 170);
            card.Controls.Add(statusCard);

            Label statusLabel = MakeLabel("TAILSCALE STATUS", 7.5f, FontStyle.Bold, Theme.TextDim);
            statusLabel.SetBounds(14, 10, 220, 14);
            statusLabel.BackColor = statusCard.FillColor;
            statusCard.Controls.Add(statusLabel);

            _statusPill = new StatusPill();
            _statusPill.SetBounds(14, 28, 190, 30);
            _statusPill.Set("Checking", Theme.TextDim, false);
            statusCard.Controls.Add(_statusPill);

            _statusDetail = MakeLabel("Checking Tailscale...", 8.5f, FontStyle.Regular, Theme.TextMuted);
            _statusDetail.SetBounds(214, 28, 356, 30);
            _statusDetail.BackColor = statusCard.FillColor;
            statusCard.Controls.Add(_statusDetail);

            _hostValue = MakeLabel("Host  -", 8.25f, FontStyle.Regular, Theme.TextMuted, true);
            _hostValue.SetBounds(14, 68, 556, 18);
            _hostValue.BackColor = statusCard.FillColor;
            statusCard.Controls.Add(_hostValue);

            _funnelValue = MakeLabel("Funnel  -", 8.25f, FontStyle.Regular, Theme.TextMuted, true);
            _funnelValue.SetBounds(14, 88, 556, 18);
            _funnelValue.BackColor = statusCard.FillColor;
            statusCard.Controls.Add(_funnelValue);

            _message = MakeLabel(string.Empty, 8.25f, FontStyle.Regular, Theme.TextDim);
            _message.SetBounds(14, 112, 556, 48);
            _message.BackColor = statusCard.FillColor;
            statusCard.Controls.Add(_message);

            _installButton = MakeButton(card, "Install Tailscale", ButtonKind.Secondary, 18, 344, 132, 36, InstallTailscale);
            _connectButton = MakeButton(card, "Connect", ButtonKind.Secondary, 158, 344, 118, 36, ConnectTailscale);
            _refreshButton = MakeButton(card, "Refresh", ButtonKind.Ghost, 284, 344, 86, 36, async () => await RefreshStatusAsync());
            _enableButton = MakeButton(card, "Enable Funnel", ButtonKind.Primary, 378, 344, 126, 36, EnableFunnel);
            _disableButton = MakeButton(card, "Disable", ButtonKind.Ghost, 512, 344, 82, 36, DisableFunnel);

            AppButton adminLink = MakeLink(card, "Open Tailscale admin console", 18, 388, 210, () => TailscaleManager.OpenAdminConsole());
            adminLink.IconPainter = null;
            AppButton docsLink = MakeLink(card, "Funnel help", 236, 388, 100, () => TailscaleManager.OpenFunnelDocs());
            docsLink.IconPainter = null;

            Label warning = MakeLabel(
                "Funnel is fine for manifests and metadata, but Tailscale applies bandwidth limits to sustained video streaming. " +
                "Performance may vary while watching.",
                8f, FontStyle.Regular, Theme.Warning);
            warning.SetBounds(18, 416, 584, 36);
            card.Controls.Add(warning);

            var urlCard = new Card
            {
                FillColor = Theme.CardSoft,
                StrokeColor = Theme.BorderSoft,
                Radius = 10,
            };
            urlCard.SetBounds(18, 460, 584, 122);
            card.Controls.Add(urlCard);

            Label publicLabel = MakeLabel("PUBLIC HOSTNAME", 7.5f, FontStyle.Bold, Theme.TextDim);
            publicLabel.SetBounds(14, 10, 240, 14);
            publicLabel.BackColor = urlCard.FillColor;
            urlCard.Controls.Add(publicLabel);

            _publicValue = MakeLabel("Not available yet", 9f, FontStyle.Regular, Theme.Text, true);
            _publicValue.SetBounds(14, 28, 556, 20);
            _publicValue.BackColor = urlCard.FillColor;
            urlCard.Controls.Add(_publicValue);

            Label manifestLabel = MakeLabel("STREMIO MANIFEST", 7.5f, FontStyle.Bold, Theme.TextDim);
            manifestLabel.SetBounds(14, 54, 240, 14);
            manifestLabel.BackColor = urlCard.FillColor;
            urlCard.Controls.Add(manifestLabel);

            _manifestValue = MakeLabel("Start the relay and enable Funnel", 9f, FontStyle.Regular, Theme.Text, true);
            _manifestValue.SetBounds(14, 72, 556, 20);
            _manifestValue.BackColor = urlCard.FillColor;
            urlCard.Controls.Add(_manifestValue);

            Label stableLabel = MakeLabel("Stable URL - Stremio keeps this link after restarts.",
                7.75f, FontStyle.Regular, Theme.TextDim);
            stableLabel.SetBounds(14, 96, 556, 16);
            stableLabel.BackColor = urlCard.FillColor;
            urlCard.Controls.Add(stableLabel);

            _copyButton = MakeButton(card, "Copy link", ButtonKind.Secondary, 18, 598, 118, 40, CopyLink);
            _stremioButton = MakeButton(card, "Install in Stremio", ButtonKind.Primary, 144, 598, 152, 40, InstallInStremio);
            _closeButton = MakeButton(card, "Close", ButtonKind.Ghost, 484, 598, 118, 40, CloseDialog);

            Shown += async (s, e) => await RefreshStatusAsync();
            UpdateModeCards();
            UpdateUi();
        }

        protected override void OnHandleCreated(EventArgs e)
        {
            base.OnHandleCreated(e);
            UiChrome.EnableDarkTitleBar(this);
        }

        // ------------------------------------------------------------- build

        private Card MakeModeCard(Card parent, int x, int y, string title, string subtitle,
            PublicAccessMode mode, out RadioDot dot, out Label titleLabel, out Label subLabel)
        {
            var card = new Card
            {
                FillColor = Theme.CardSoft,
                StrokeColor = Theme.Border,
                Radius = 10,
                Cursor = Cursors.Hand,
            };
            card.SetBounds(x, y, 288, 76);
            parent.Controls.Add(card);

            dot = new RadioDot { BackColor = card.FillColor };
            dot.SetBounds(14, 27, 20, 20);
            dot.Enabled = false;
            card.Controls.Add(dot);

            titleLabel = MakeLabel(title, 9.5f, FontStyle.Bold, Theme.Text);
            titleLabel.SetBounds(44, 10, 230, 22);
            titleLabel.Cursor = Cursors.Hand;
            titleLabel.BackColor = card.FillColor;
            card.Controls.Add(titleLabel);

            subLabel = MakeLabel(subtitle, 8f, FontStyle.Regular, Theme.TextDim);
            subLabel.SetBounds(44, 32, 230, 32);
            subLabel.Cursor = Cursors.Hand;
            subLabel.BackColor = card.FillColor;
            card.Controls.Add(subLabel);

            EventHandler select = (s, e) => SelectMode(mode);
            card.Click += select;
            titleLabel.Click += select;
            subLabel.Click += select;
            return card;
        }

        private AppButton MakeButton(Card parent, string text, ButtonKind kind, int x, int y, int w, int h, Action action)
        {
            var button = new AppButton
            {
                Text = text,
                Kind = kind,
                Backplate = Theme.Card,
            };
            button.SetBounds(x, y, w, h);
            button.Click += (s, e) => { if (!_busy) action(); };
            parent.Controls.Add(button);
            return button;
        }

        private AppButton MakeLink(Card parent, string text, int x, int y, int w, Action action)
        {
            var button = new AppButton
            {
                Text = text,
                Kind = ButtonKind.Link,
                Backplate = Theme.Card,
                Font = Theme.FontSmall,
            };
            button.SetBounds(x, y, w, 22);
            button.Click += (s, e) => { if (!_busy) action(); };
            parent.Controls.Add(button);
            return button;
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

        // ------------------------------------------------------------- modes

        private void SelectMode(PublicAccessMode mode)
        {
            if (_busy || _owner.AccessMode == mode) return;
            _owner.ApplyAccessMode(mode);
            _status = null;
            _funnel = null;
            _message.Text = string.Empty;
            UpdateModeCards();
            UpdateUi();

            if (_owner.RelayActive)
            {
                DialogResult restart = MessageBox.Show(this,
                    "Restart the relay now to apply " + mode.DisplayName() + "?",
                    "Public access", MessageBoxButtons.YesNo, MessageBoxIcon.Question);
                if (restart == DialogResult.Yes)
                {
                    RestartRelay();
                    return;
                }
            }
            _ = RefreshStatusAsync();
        }

        private void RestartRelay()
        {
            RunBusy("Restarting the relay...", async () =>
            {
                await _owner.RestartRelayForAccessAsync();
                await RefreshStatusAsync();
            });
        }

        private void UpdateModeCards()
        {
            bool tailscale = _owner.AccessMode == PublicAccessMode.Tailscale;
            _cloudflareDot.Selected = !tailscale;
            _tailscaleDot.Selected = tailscale;
            _cloudflareCard.FillColor = tailscale ? Theme.CardSoft : Color.FromArgb(22, 42, 32);
            _cloudflareCard.StrokeColor = tailscale ? Theme.Border : Theme.Alpha(Theme.Accent, 150);
            _tailscaleCard.FillColor = tailscale ? Color.FromArgb(22, 42, 32) : Theme.CardSoft;
            _tailscaleCard.StrokeColor = tailscale ? Theme.Alpha(Theme.Accent, 150) : Theme.Border;
            _cloudflareTitle.BackColor = _cloudflareCard.FillColor;
            _cloudflareSub.BackColor = _cloudflareCard.FillColor;
            _tailscaleTitle.BackColor = _tailscaleCard.FillColor;
            _tailscaleSub.BackColor = _tailscaleCard.FillColor;
            _cloudflareCard.Invalidate();
            _tailscaleCard.Invalidate();
            _cloudflareDot.Invalidate();
            _tailscaleDot.Invalidate();
        }

        // ------------------------------------------------------------- status

        private async Task RefreshStatusAsync()
        {
            if (_busy)
            {
                _refreshPending = true;
                return;
            }
            _status = null;
            _funnel = null;
            SetBusy(true, "Checking Tailscale...");
            try
            {
                _status = await _tailscale.GetStatusAsync();
                if (_status.Installed) _funnel = await _tailscale.GetFunnelAsync();
            }
            catch (Exception ex)
            {
                _message.Text = ex.Message;
                _message.ForeColor = Theme.Danger;
            }
            finally
            {
                SetBusy(false, null);
                UpdateUi();
                if (_refreshPending && !IsDisposed)
                {
                    _refreshPending = false;
                    await Task.Delay(250);
                    await RefreshStatusAsync();
                }
            }
        }

        private void UpdateUi()
        {
            bool tailscale = _owner.AccessMode == PublicAccessMode.Tailscale;
            bool tsInstalled = _status != null && _status.Installed;
            bool connected = _status != null && _status.Connected;
            bool funnelActive = tailscale && _funnel != null && _funnel.Configured && _funnel.Enabled &&
                                _funnel.Matches(_owner.RelayPort);
            bool conflict = tailscale && _funnel != null && _funnel.Configured &&
                            !_funnel.Matches(_owner.RelayPort) && !string.IsNullOrEmpty(_funnel.Target);

            _publicUrl = string.Empty;
            _manifestUrl = string.Empty;

            if (!tailscale)
            {
                _statusPill.Set("Cloudflare", Theme.IndigoSoft, false);
                _statusDetail.Text = "Temporary hostname, refreshed on every start.";
                _hostValue.Text = "Host  Tailscale is not used in this mode";
                _funnelValue.Text = "Funnel  -";
                _message.Text = "Quick Tunnel needs cloudflared and changes its URL on every start. " +
                                "Switch to Tailscale Funnel for a permanent URL.";
                _message.ForeColor = Theme.TextDim;
                _installButton.Enabled = false;
                _connectButton.Enabled = false;
                _refreshButton.Enabled = false;
                _enableButton.Enabled = false;
                _disableButton.Enabled = false;

                if (!string.IsNullOrEmpty(_owner.CurrentManifest))
                {
                    _manifestUrl = _owner.CurrentManifest;
                    _publicUrl = OriginOf(_manifestUrl);
                }
            }
            else if (_status == null)
            {
                _statusPill.Set("Checking...", Theme.WarningSoft, true);
                _statusDetail.Text = "Reading Tailscale status...";
                _hostValue.Text = "Host  -";
                _funnelValue.Text = "Funnel  -";
                _message.Text = "Checking the Tailscale service and Funnel configuration.";
                _message.ForeColor = Theme.TextDim;
                _installButton.Enabled = false;
                _connectButton.Enabled = false;
                _refreshButton.Enabled = false;
                _enableButton.Enabled = false;
                _disableButton.Enabled = false;
            }
            else if (!tsInstalled)
            {
                _statusPill.Set("Not installed", Theme.Danger, false);
                _statusDetail.Text = "Tailscale is not installed on this PC.";
                _hostValue.Text = "Host  -";
                _funnelValue.Text = "Funnel  -";
                _message.Text = "Install Tailscale, sign in once, then click Refresh. The installer may ask for administrator rights.";
                _message.ForeColor = Theme.Warning;
                _installButton.Enabled = true;
                _connectButton.Enabled = false;
                _refreshButton.Enabled = true;
                _enableButton.Enabled = false;
                _disableButton.Enabled = false;
            }
            else if (!_status.ServiceRunning)
            {
                _statusPill.Set("Service stopped", Theme.Danger, false);
                _statusDetail.Text = "The Tailscale Windows service is not running.";
                _hostValue.Text = "Host  " + (_status.Hostname ?? "-");
                _funnelValue.Text = "Funnel  -";
                _message.Text = "Start the Tailscale app (or the Tailscale service), then click Refresh.";
                _message.ForeColor = Theme.Warning;
                _installButton.Enabled = false;
                _connectButton.Enabled = false;
                _refreshButton.Enabled = true;
                _enableButton.Enabled = false;
                _disableButton.Enabled = false;
            }
            else if (!connected)
            {
                _statusPill.Set(_status.NeedsLogin ? "Sign-in needed" : "Not connected", Theme.WarningSoft, false);
                _statusDetail.Text = _status.NeedsLogin
                    ? "Waiting for Tailscale authentication."
                    : "Tailscale is installed but offline.";
                _hostValue.Text = "Host  " + (_status.Hostname ?? "-");
                _funnelValue.Text = "Funnel  -";
                _message.Text = "Click Connect. Tailscale opens the browser; finish sign-in and this window updates automatically.";
                _message.ForeColor = Theme.TextMuted;
                _installButton.Enabled = false;
                _connectButton.Enabled = true;
                _refreshButton.Enabled = true;
                _enableButton.Enabled = false;
                _disableButton.Enabled = false;
            }
            else
            {
                string host = (_funnel != null && !string.IsNullOrEmpty(_funnel.Hostname))
                    ? _funnel.Hostname
                    : _status.Hostname;

                if (funnelActive)
                {
                    _statusPill.Set("Funnel active", Theme.Accent, true);
                    _statusDetail.Text = "Connected - stable public URL is live.";
                    _publicUrl = "https://" + host;
                    _manifestUrl = _publicUrl + "/manifest.json";
                    _message.Text = "This URL stays the same after restarts. Keep the app running in the tray to stay online.";
                    _message.ForeColor = Theme.Accent;
                }
                else if (conflict)
                {
                    _statusPill.Set("Funnel conflict", Theme.Danger, false);
                    _statusDetail.Text = "Port 443 already serves another service.";
                    _message.Text = "Port 443 is already used by " + _funnel.Target +
                                    ". DhakaFlix will not replace it silently; review it in the admin console first.";
                    _message.ForeColor = Theme.Danger;
                }
                else
                {
                    _statusPill.Set("Funnel off", Theme.WarningSoft, false);
                    _statusDetail.Text = "Connected - Funnel is not enabled yet.";
                    _message.Text = "Click Enable Funnel to publish https://" + (host ?? "your-host") +
                                    " for this relay. The relay must be running.";
                    _message.ForeColor = Theme.TextMuted;
                }

                _hostValue.Text = "Host  " + (host ?? "-");
                _funnelValue.Text = _funnel == null || !_funnel.Configured
                    ? "Funnel  not configured"
                    : "Funnel  443 -> " + (_funnel.Target ?? "(unknown)");

                _installButton.Enabled = false;
                _connectButton.Enabled = false;
                _refreshButton.Enabled = true;
                _disableButton.Enabled = _funnel != null && _funnel.Configured;
                _enableButton.Enabled = !funnelActive && (conflict || _funnel == null || !_funnel.Configured ||
                    !_funnel.Matches(_owner.RelayPort) || !_funnel.Enabled);

                if (conflict)
                {
                    _enableButton.Text = "Replace Funnel";
                    _enableButton.Kind = ButtonKind.Danger;
                }
                else
                {
                    _enableButton.Text = "Enable Funnel";
                    _enableButton.Kind = ButtonKind.Primary;
                }
                _enableButton.Invalidate();

                if (!funnelActive && !_status.MagicDnsEnabled)
                {
                    _enableButton.Enabled = false;
                    _message.Text = "MagicDNS is disabled for this tailnet. Enable MagicDNS in the Tailscale admin console, then Refresh.";
                    _message.ForeColor = Theme.Warning;
                }
                else if (!funnelActive && !_status.CertAvailable)
                {
                    _enableButton.Enabled = false;
                    _message.Text = "Tailscale has not issued an HTTPS certificate domain yet. Enable HTTPS certificates in the admin console, then Refresh.";
                    _message.ForeColor = Theme.Warning;
                }

                if (tailscale && _owner.RelayActive && _owner.RunningMode != PublicAccessMode.Tailscale)
                {
                    _enableButton.Enabled = false;
                    _message.Text = "The relay is still running in Cloudflare mode. Restart it before enabling Funnel.";
                    _message.ForeColor = Theme.Warning;
                }
            }

            _publicValue.Text = string.IsNullOrEmpty(_publicUrl) ? "Not available yet" : _publicUrl;
            _manifestValue.Text = string.IsNullOrEmpty(_manifestUrl) ? "Start the relay and enable Funnel" : _manifestUrl;
            _publicValue.ForeColor = string.IsNullOrEmpty(_publicUrl) ? Theme.TextDim : Theme.Text;
            _manifestValue.ForeColor = string.IsNullOrEmpty(_manifestUrl) ? Theme.TextDim : Theme.Text;
            _copyButton.Enabled = !_busy && !string.IsNullOrEmpty(_manifestUrl);
            _stremioButton.Enabled = !_busy && !string.IsNullOrEmpty(_manifestUrl);

            _installButton.Invalidate();
            _connectButton.Invalidate();
            _enableButton.Invalidate();
            _disableButton.Invalidate();
            _copyButton.Invalidate();
            _stremioButton.Invalidate();
        }

        private static string OriginOf(string manifest)
        {
            if (string.IsNullOrEmpty(manifest)) return string.Empty;
            const string suffix = "/manifest.json";
            return manifest.EndsWith(suffix, StringComparison.OrdinalIgnoreCase)
                ? manifest.Substring(0, manifest.Length - suffix.Length)
                : manifest;
        }

        // ------------------------------------------------------------- actions

        private void InstallTailscale()
        {
            TailscaleManager.OpenDownloadPage();
            _message.Text = "Download Tailscale, install it, sign in, then click Refresh.";
            _message.ForeColor = Theme.TextMuted;
        }

        private void ConnectTailscale()
        {
            if (_busy) return;
            _ = ConnectAsync();
        }

        private async Task ConnectAsync()
        {
            SetBusy(true, "Waiting for authentication in your browser...");
            _owner.LogAccess("Tailscale connect requested", LogLevel.Info);
            try
            {
                await _tailscale.ConnectAsync((text, level) => _owner.LogAccess(text, level), CancellationToken.None);
                _message.Text = "Tailscale is connected.";
                _message.ForeColor = Theme.Accent;
            }
            catch (TailscaleException ex)
            {
                _message.Text = ex.Message;
                _message.ForeColor = Theme.Danger;
                _owner.LogAccess(ex.Message, LogLevel.Error);
            }
            catch (Exception ex)
            {
                _message.Text = ex.Message;
                _message.ForeColor = Theme.Danger;
            }
            finally
            {
                SetBusy(false, null);
                await RefreshStatusAsync();
            }
        }

        private void EnableFunnel()
        {
            if (_busy) return;
            _ = EnableFunnelAsync();
        }

        private async Task EnableFunnelAsync()
        {
            bool conflict = _funnel != null && _funnel.Configured &&
                            !_funnel.Matches(_owner.RelayPort) && !string.IsNullOrEmpty(_funnel.Target);

            if (!_owner.RelayActive)
            {
                DialogResult start = MessageBox.Show(this,
                    "The local relay must be running before Funnel can publish it." + Environment.NewLine + Environment.NewLine +
                    "Start the relay now?",
                    "Start relay", MessageBoxButtons.YesNo, MessageBoxIcon.Question);
                if (start != DialogResult.Yes) return;
                SetBusy(true, "Starting the relay...");
                bool ok = await _owner.StartRelayForAccessAsync();
                if (!ok)
                {
                    SetBusy(false, null);
                    _message.Text = "The relay did not start. Check the dashboard log.";
                    _message.ForeColor = Theme.Danger;
                    return;
                }
                SetBusy(false, null);
            }

            if (_owner.RunningMode != PublicAccessMode.Tailscale)
            {
                DialogResult restart = MessageBox.Show(this,
                    "The relay is running in Cloudflare mode. Restart it in Tailscale mode first?" + Environment.NewLine + Environment.NewLine +
                    "This stops the current relay briefly.",
                    "Restart relay", MessageBoxButtons.YesNo, MessageBoxIcon.Question);
                if (restart != DialogResult.Yes) return;
                SetBusy(true, "Restarting the relay in Tailscale mode...");
                await _owner.RestartRelayForAccessAsync();
                SetBusy(false, null);
                if (_owner.RunningMode != PublicAccessMode.Tailscale)
                {
                    _message.Text = "The relay could not restart in Tailscale mode.";
                    _message.ForeColor = Theme.Danger;
                    return;
                }
            }

            if (conflict)
            {
                DialogResult replace = MessageBox.Show(this,
                    "Port 443 is already serving " + _funnel.Target + "." + Environment.NewLine + Environment.NewLine +
                    "Replace that Funnel listener with DhakaFlix? This changes the Tailscale serve configuration for port 443.",
                    "Replace Funnel listener", MessageBoxButtons.YesNo, MessageBoxIcon.Warning);
                if (replace != DialogResult.Yes) return;
            }

            SetBusy(true, "Enabling Tailscale Funnel...");
            _owner.LogAccess("Enabling Tailscale Funnel on port 443", LogLevel.Info);
            try
            {
                TailscaleFunnelInfo funnel = await _tailscale.EnableFunnelAsync(
                    _owner.RelayPort, (text, level) => _owner.LogAccess(text, level), conflict);
                string host = !string.IsNullOrEmpty(funnel.Hostname) ? funnel.Hostname : (_status != null ? _status.Hostname : null);
                if (!string.IsNullOrEmpty(host))
                {
                    _owner.ApplyTailscaleHostname(host);
                    _publicUrl = "https://" + host;
                    _manifestUrl = _publicUrl + "/manifest.json";
                }
                _owner.RefreshDashboardAccess();
                _message.Text = "Funnel is active. Copy the Stremio manifest or install it now.";
                _message.ForeColor = Theme.Accent;
            }
            catch (TailscaleException ex)
            {
                _message.Text = ex.Message;
                _message.ForeColor = Theme.Danger;
                _owner.LogAccess(ex.Message, LogLevel.Error);
                if (ex.Issue == TailscaleIssue.FunnelPolicy || ex.Issue == TailscaleIssue.HttpsCertificate)
                    TailscaleManager.OpenAdminConsole();
            }
            catch (Exception ex)
            {
                _message.Text = ex.Message;
                _message.ForeColor = Theme.Danger;
            }
            finally
            {
                SetBusy(false, null);
                await RefreshStatusAsync();
            }
        }

        private void DisableFunnel()
        {
            if (_busy) return;
            DialogResult confirm = MessageBox.Show(this,
                "Disable the Funnel listener for DhakaFlix?" + Environment.NewLine + Environment.NewLine +
                "The stable public URL stops working, but the relay keeps running locally.",
                "Disable Funnel", MessageBoxButtons.YesNo, MessageBoxIcon.Question);
            if (confirm != DialogResult.Yes) return;
            _ = DisableFunnelAsync();
        }

        private async Task DisableFunnelAsync()
        {
            SetBusy(true, "Disabling Funnel...");
            try
            {
                await _tailscale.DisableFunnelAsync(_owner.RelayPort, (text, level) => _owner.LogAccess(text, level));
                _message.Text = "Funnel is disabled.";
                _message.ForeColor = Theme.TextMuted;
            }
            catch (TailscaleException ex)
            {
                _message.Text = ex.Message;
                _message.ForeColor = Theme.Danger;
            }
            finally
            {
                SetBusy(false, null);
                await RefreshStatusAsync();
            }
        }

        private void CopyLink()
        {
            if (string.IsNullOrEmpty(_manifestUrl)) return;
            try
            {
                Clipboard.SetText(_manifestUrl);
                _copyButton.Text = "Copied";
                var timer = new System.Windows.Forms.Timer { Interval = 1500 };
                timer.Tick += (s, e) =>
                {
                    timer.Stop();
                    timer.Dispose();
                    if (!IsDisposed) _copyButton.Text = "Copy link";
                };
                timer.Start();
            }
            catch
            {
                _message.Text = "The clipboard is busy. Select the manifest and copy it manually.";
                _message.ForeColor = Theme.Warning;
            }
        }

        private void InstallInStremio()
        {
            if (string.IsNullOrEmpty(_manifestUrl)) return;
            string error;
            if (!StremioHelper.OpenInstall(_manifestUrl, out error))
            {
                _message.Text = "Stremio could not be opened: " + error;
                _message.ForeColor = Theme.Danger;
            }
        }

        private void CloseDialog()
        {
            if (_busy) return;
            DialogResult = DialogResult.OK;
            Close();
        }

        // ------------------------------------------------------------- busy

        private void SetBusy(bool busy, string message)
        {
            _busy = busy;
            _installButton.Enabled = !busy && _status != null && !_status.Installed;
            _connectButton.Enabled = !busy;
            _refreshButton.Enabled = !busy;
            _enableButton.Enabled = !busy;
            _disableButton.Enabled = !busy;
            _copyButton.Enabled = !busy && !string.IsNullOrEmpty(_manifestUrl);
            _stremioButton.Enabled = !busy && !string.IsNullOrEmpty(_manifestUrl);
            _closeButton.Enabled = !busy;

            if (busy)
            {
                _statusPill.Set("Working...", Theme.WarningSoft, true);
                if (!string.IsNullOrEmpty(message)) _statusDetail.Text = message;
            }
        }

        private async void RunBusy(string message, Func<Task> action)
        {
            if (_busy) return;
            SetBusy(true, message);
            try
            {
                await action();
            }
            catch (Exception ex)
            {
                _message.Text = ex.Message;
                _message.ForeColor = Theme.Danger;
            }
            finally
            {
                SetBusy(false, null);
                UpdateUi();
            }
        }

        protected override void Dispose(bool disposing)
        {
            base.Dispose(disposing);
        }
    }
}
