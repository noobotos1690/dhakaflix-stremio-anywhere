using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.ServiceProcess;
using System.Text;
using System.Text.RegularExpressions;
using System.Threading;
using System.Threading.Tasks;
using System.Web.Script.Serialization;

namespace DhakaFlixRelay
{
    internal enum TailscaleIssue
    {
        None,
        NotInstalled,
        ServiceStopped,
        NotConnected,
        AuthRequired,
        FunnelPolicy,
        HttpsCertificate,
        FunnelConflict,
        LocalRelayDown,
        PublicHealth,
        CliError
    }

    internal sealed class TailscaleException : Exception
    {
        public TailscaleIssue Issue { get; private set; }

        public TailscaleException(TailscaleIssue issue, string message) : base(message)
        {
            Issue = issue;
        }
    }

    internal sealed class TailscaleStatusInfo
    {
        public bool Installed;
        public string Executable;
        public bool ServiceRunning;
        public string BackendState;
        public bool Connected;
        public bool NeedsLogin;
        public string AuthUrl;
        public string Hostname;
        public string MagicDnsSuffix;
        public bool MagicDnsEnabled;
        public bool CertAvailable;
        public string Version;
        public string Error;
    }

    internal sealed class TailscaleFunnelInfo
    {
        public bool Configured;
        public bool Enabled;
        public string Hostname;
        public string Target;
        public bool OtherListeners;
        public string Error;

        public bool Matches(int port)
        {
            return !string.IsNullOrEmpty(Target) &&
                   string.Equals(Target.TrimEnd('/'), "http://127.0.0.1:" + port, StringComparison.OrdinalIgnoreCase);
        }
    }

    /// <summary>
    /// Wraps the Tailscale CLI. All calls use the absolute executable path so
    /// the app never depends on PATH, and every wait is asynchronous so the
    /// WinForms thread stays responsive.
    /// </summary>
    internal sealed class TailscaleManager
    {
        public const string DownloadUrl = "https://tailscale.com/download/windows";
        public const string AdminConsoleUrl = "https://login.tailscale.com/admin/machines";
        public const string FunnelDocsUrl = "https://tailscale.com/kb/1223/funnel";

        private static readonly Regex UrlPattern = new Regex(@"https://[^\s""']+", RegexOptions.IgnoreCase | RegexOptions.Compiled);
        private static string _cachedExecutable;

        public string Executable
        {
            get { return FindExecutable(); }
        }

        public bool IsInstalled
        {
            get { return FindExecutable() != null; }
        }

        public static string FindExecutable()
        {
            if (!string.IsNullOrEmpty(_cachedExecutable) && File.Exists(_cachedExecutable))
                return _cachedExecutable;

            if (string.Equals(Environment.GetEnvironmentVariable("DHAKAFLIX_TAILSCALE_DISABLED"), "1", StringComparison.Ordinal))
                return null;

            string overridePath = Environment.GetEnvironmentVariable("DHAKAFLIX_TAILSCALE_PATH");
            if (!string.IsNullOrEmpty(overridePath) && File.Exists(overridePath))
            {
                _cachedExecutable = overridePath;
                return overridePath;
            }

            string programFiles = Environment.GetFolderPath(Environment.SpecialFolder.ProgramFiles);
            string programFilesX86 = Environment.GetFolderPath(Environment.SpecialFolder.ProgramFilesX86);
            string local = Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData);

            string[] candidates =
            {
                Path.Combine(programFiles, "Tailscale", "tailscale.exe"),
                Path.Combine(programFilesX86, "Tailscale", "tailscale.exe"),
                Path.Combine(local, "Tailscale", "tailscale.exe"),
                ProcessRunner.Which("tailscale.exe"),
            };

            foreach (string candidate in candidates)
            {
                try
                {
                    if (!string.IsNullOrEmpty(candidate) && File.Exists(candidate))
                    {
                        _cachedExecutable = candidate;
                        return candidate;
                    }
                }
                catch
                {
                    // keep looking
                }
            }
            return null;
        }

        public static void ResetExecutableCache()
        {
            _cachedExecutable = null;
        }

        // ------------------------------------------------------------- status

        public async Task<TailscaleStatusInfo> GetStatusAsync()
        {
            var info = new TailscaleStatusInfo();
            info.Executable = FindExecutable();
            info.Installed = info.Executable != null;
            if (!info.Installed)
            {
                info.Error = "Tailscale is not installed.";
                return info;
            }

            info.ServiceRunning = IsServiceRunning();
            ProcessResult result = await Task.Run(() =>
                ProcessRunner.CaptureResult(info.Executable, "status --json", Environment.CurrentDirectory, 12000))
                .ConfigureAwait(false);

            if (string.IsNullOrWhiteSpace(result.StdOut))
            {
                info.Error = FirstLine(result.StdErr) ?? (info.ServiceRunning
                    ? "Tailscale did not return a status."
                    : "The Tailscale service is not running.");
                return info;
            }

            try
            {
                TailscaleStatusInfo parsed = ParseStatusJson(result.StdOut);
                parsed.Installed = true;
                parsed.Executable = info.Executable;
                parsed.ServiceRunning = info.ServiceRunning;
                if (!parsed.ServiceRunning && !parsed.Connected)
                    parsed.Error = "The Tailscale service is not running.";
                return parsed;
            }
            catch (Exception ex)
            {
                info.Error = "Could not read Tailscale status: " + ex.Message;
                return info;
            }
        }

        internal static TailscaleStatusInfo ParseStatusJson(string json)
        {
            var serializer = new JavaScriptSerializer();
            var root = serializer.DeserializeObject(json) as Dictionary<string, object>;
            if (root == null) throw new InvalidOperationException("status JSON was empty");

            var info = new TailscaleStatusInfo();
            info.BackendState = GetString(root, "BackendState");
            info.AuthUrl = GetString(root, "AuthURL");
            info.MagicDnsSuffix = GetString(root, "MagicDNSSuffix");
            info.Version = GetString(root, "Version");
            info.NeedsLogin = string.Equals(info.BackendState, "NeedsLogin", StringComparison.OrdinalIgnoreCase) ||
                              !string.IsNullOrEmpty(info.AuthUrl);

            var self = GetDictionary(root, "Self");
            string dnsName = self != null ? GetString(self, "DNSName") : null;


            string certHost = null;
            var certDomains = GetArray(root, "CertDomains");
            if (certDomains != null)
            {
                foreach (object item in certDomains)
                {
                    string value = item as string;
                    if (!string.IsNullOrEmpty(value))
                    {
                        certHost = value;
                        break;
                    }
                }
            }
            info.CertAvailable = !string.IsNullOrEmpty(certHost);
            info.Hostname = NormalizeHost(certHost ?? dnsName);
            info.Connected = string.Equals(info.BackendState, "Running", StringComparison.OrdinalIgnoreCase) &&
                             !string.IsNullOrEmpty(dnsName);

            var tailnet = GetDictionary(root, "CurrentTailnet");
            if (tailnet != null)
            {
                info.MagicDnsEnabled = GetBool(tailnet, "MagicDNSEnabled") ||
                                       !string.IsNullOrEmpty(info.MagicDnsSuffix);
            }
            else
            {
                info.MagicDnsEnabled = !string.IsNullOrEmpty(info.MagicDnsSuffix);
            }

            var health = GetArray(root, "Health");
            if (health != null && health.Length > 0)
            {
                var warnings = new List<string>();
                foreach (object item in health)
                {
                    string text = item as string;
                    if (!string.IsNullOrEmpty(text)) warnings.Add(text);
                }
                if (warnings.Count > 0) info.Error = string.Join(" ", warnings.ToArray());
            }

            if (info.NeedsLogin && info.Connected)
                info.Connected = false;

            return info;
        }

        private static bool IsServiceRunning()
        {
            try
            {
                using (var controller = new ServiceController("Tailscale"))
                {
                    return controller.Status == ServiceControllerStatus.Running;
                }
            }
            catch
            {
                // If the service name is unavailable, trust the CLI result.
                return true;
            }
        }

        // ------------------------------------------------------------- funnel

        public async Task<TailscaleFunnelInfo> GetFunnelAsync()
        {
            string exe = FindExecutable();
            var info = new TailscaleFunnelInfo();
            if (exe == null)
            {
                info.Error = "Tailscale is not installed.";
                return info;
            }

            ProcessResult result = await Task.Run(() =>
                ProcessRunner.CaptureResult(exe, "funnel status --json", Environment.CurrentDirectory, 12000))
                .ConfigureAwait(false);

            string text = (result.StdOut ?? string.Empty).Trim();
            if (text.IndexOf("no serve config", StringComparison.OrdinalIgnoreCase) >= 0)
                return info;
            if (text.Length == 0 || text == "{}")
            {
                if (result.ExitCode != 0 && !string.IsNullOrEmpty(result.StdErr) &&
                    result.StdErr.IndexOf("no serve config", StringComparison.OrdinalIgnoreCase) < 0)
                {
                    info.Error = FirstLine(result.StdErr);
                }
                return info;
            }

            try
            {
                return ParseFunnelJson(text);
            }
            catch (Exception ex)
            {
                info.Error = "Could not read Funnel status: " + ex.Message;
                return info;
            }
        }

        internal static TailscaleFunnelInfo ParseFunnelJson(string json)
        {
            var serializer = new JavaScriptSerializer();
            var root = serializer.DeserializeObject(json) as Dictionary<string, object>;
            var info = new TailscaleFunnelInfo();
            if (root == null) return info;

            var web = GetDictionary(root, "Web");
            var allow = GetDictionary(root, "AllowFunnel");
            if (web == null || web.Count == 0) return info;

            foreach (var entry in web)
            {
                string key = entry.Key;
                string host = key;
                int colon = key.LastIndexOf(':');
                if (colon > 0) host = key.Substring(0, colon);

                var webEntry = entry.Value as Dictionary<string, object>;
                if (webEntry == null) continue;

                var handlers = webEntry.ContainsKey("Handlers")
                    ? webEntry["Handlers"] as Dictionary<string, object>
                    : webEntry;
                if (handlers == null || handlers.Count == 0) continue;

                string proxy = null;
                bool onlyRoot = true;
                foreach (var handler in handlers)
                {
                    if (!string.Equals(handler.Key, "/", StringComparison.Ordinal)) onlyRoot = false;
                    var details = handler.Value as Dictionary<string, object>;
                    if (details != null)
                    {
                        string value = GetString(details, "Proxy");
                        if (!string.IsNullOrEmpty(value) && proxy == null) proxy = value;
                    }
                }

                info.Configured = true;
                info.Hostname = host;
                info.Target = proxy;
                info.OtherListeners = !onlyRoot || web.Count > 1;

                if (allow != null)
                {
                    object enabled;
                    if (allow.TryGetValue(key, out enabled) && enabled is bool)
                        info.Enabled = (bool)enabled;
                }
                break;
            }

            return info;
        }

        // ------------------------------------------------------------- enable

        public async Task<TailscaleFunnelInfo> EnableFunnelAsync(int port, Action<string, LogLevel> log, bool replaceExisting = false)
        {
            string exe = FindExecutable();
            if (exe == null)
                throw new TailscaleException(TailscaleIssue.NotInstalled, "Tailscale is not installed.");

            string target = "http://127.0.0.1:" + port;
            TailscaleFunnelInfo current = await GetFunnelAsync().ConfigureAwait(false);

            if (current.Configured && current.Enabled && current.Matches(port))
                return current;

            if (current.Configured && !string.IsNullOrEmpty(current.Target) && !current.Matches(port))
            {
                if (!replaceExisting)
                {
                    throw new TailscaleException(TailscaleIssue.FunnelConflict,
                        "Port 443 is already used by another Funnel service (" + current.Target +
                        "). DhakaFlix will not replace it automatically.");
                }
                if (log != null) log("Replacing the existing Funnel listener " + current.Target, LogLevel.Warn);
            }

            if (log != null) log("Configuring Tailscale Funnel for " + target, LogLevel.Info);
            ProcessResult result = await Task.Run(() => ProcessRunner.CaptureResult(
                exe,
                "funnel --bg --yes --https=443 " + target,
                Environment.CurrentDirectory,
                90000)).ConfigureAwait(false);

            if (result.ExitCode != 0)
                throw MapFunnelError(result.StdErr, result.StdOut, string.Empty);

            for (int i = 0; i < 20; i++)
            {
                await Task.Delay(1000).ConfigureAwait(false);
                TailscaleFunnelInfo status = await GetFunnelAsync().ConfigureAwait(false);
                if (status.Configured && status.Enabled && status.Matches(port))
                {
                    if (log != null) log("Funnel is active at https://" + status.Hostname, LogLevel.Info);
                    return status;
                }
            }

            throw new TailscaleException(TailscaleIssue.CliError,
                "Tailscale accepted the command but Funnel did not become active. Check the admin console.");
        }

        public async Task DisableFunnelAsync(int port, Action<string, LogLevel> log)
        {
            string exe = FindExecutable();
            if (exe == null)
                throw new TailscaleException(TailscaleIssue.NotInstalled, "Tailscale is not installed.");

            TailscaleFunnelInfo current = await GetFunnelAsync().ConfigureAwait(false);
            if (!current.Configured) return;

            if (!string.IsNullOrEmpty(current.Target) && !current.Matches(port))
            {
                throw new TailscaleException(TailscaleIssue.FunnelConflict,
                    "Funnel is currently serving " + current.Target +
                    ". DhakaFlix will not disable another service.");
            }

            if (current.OtherListeners)
            {
                throw new TailscaleException(TailscaleIssue.FunnelConflict,
                    "Port 443 has additional Funnel handlers. DhakaFlix will not disable a shared listener.");
            }

            if (log != null) log("Disabling the Funnel listener for DhakaFlix", LogLevel.Info);
            ProcessResult result = await Task.Run(() => ProcessRunner.CaptureResult(
                exe,
                "funnel --https=443 off",
                Environment.CurrentDirectory,
                30000)).ConfigureAwait(false);

            if (result.ExitCode != 0)
                throw new TailscaleException(TailscaleIssue.CliError,
                    FirstLine(result.StdErr) ?? "Tailscale could not disable Funnel.");

            for (int i = 0; i < 10; i++)
            {
                await Task.Delay(500).ConfigureAwait(false);
                TailscaleFunnelInfo status = await GetFunnelAsync().ConfigureAwait(false);
                if (!status.Configured) return;
            }
        }

        // ------------------------------------------------------------- connect

        public async Task ConnectAsync(Action<string, LogLevel> log, CancellationToken cancellation)
        {
            string exe = FindExecutable();
            if (exe == null)
                throw new TailscaleException(TailscaleIssue.NotInstalled, "Tailscale is not installed.");

            if (log != null) log("Starting Tailscale and waiting for authentication...", LogLevel.Info);

            var startInfo = new ProcessStartInfo
            {
                FileName = exe,
                Arguments = "up",
                UseShellExecute = false,
                CreateNoWindow = true,
                RedirectStandardOutput = true,
                RedirectStandardError = true,
                StandardOutputEncoding = new UTF8Encoding(false),
                StandardErrorEncoding = new UTF8Encoding(false),
                WindowStyle = ProcessWindowStyle.Hidden,
            };

            string authUrl = null;
            var errorText = new StringBuilder();
            using (var process = new Process { StartInfo = startInfo })
            {
                DataReceivedEventHandler onLine = (s, e) =>
                {
                    if (e.Data == null) return;
                    if (log != null) log(e.Data.Trim(), LogLevel.Debug);
                    Match match = UrlPattern.Match(e.Data);
                    if (match.Success && string.IsNullOrEmpty(authUrl))
                    {
                        authUrl = match.Value;
                        TryOpen(authUrl);
                    }
                };

                process.OutputDataReceived += onLine;
                process.ErrorDataReceived += (s, e) =>
                {
                    if (e.Data == null) return;
                    errorText.AppendLine(e.Data);
                    onLine(s, e);
                };

                process.Start();
                process.BeginOutputReadLine();
                process.BeginErrorReadLine();

                DateTime deadline = DateTime.UtcNow.AddSeconds(120);
                try
                {
                    while (DateTime.UtcNow < deadline)
                    {
                        cancellation.ThrowIfCancellationRequested();
                        TailscaleStatusInfo status = await GetStatusAsync().ConfigureAwait(false);
                        if (status.Connected) return;

                        if (process.HasExited && !status.Connected)
                        {
                            string detail = FirstLine(errorText.ToString());
                            throw new TailscaleException(TailscaleIssue.AuthRequired,
                                detail ?? "Tailscale did not finish signing in. Run Connect again to retry.");
                        }

                        if (string.IsNullOrEmpty(authUrl) && !string.IsNullOrEmpty(status.AuthUrl))
                        {
                            authUrl = status.AuthUrl;
                            TryOpen(authUrl);
                        }

                        await Task.Delay(2000, cancellation).ConfigureAwait(false);
                    }
                }
                finally
                {
                    try
                    {
                        if (!process.HasExited) ProcessRunner.KillTree(process);
                    }
                    catch
                    {
                        // already gone
                    }
                }
            }

            throw new TailscaleException(TailscaleIssue.AuthRequired,
                "Timed out waiting for Tailscale authentication." +
                (authUrl != null ? " Complete sign-in at " + authUrl + " and try again." : string.Empty));
        }

        public static void OpenDownloadPage()
        {
            TryOpen(DownloadUrl);
        }

        public static void OpenAdminConsole()
        {
            TryOpen(AdminConsoleUrl);
        }

        public static void OpenFunnelDocs()
        {
            TryOpen(FunnelDocsUrl);
        }

        public static void TryOpen(string url)
        {
            try
            {
                Process.Start(new ProcessStartInfo(url) { UseShellExecute = true });
            }
            catch
            {
                // caller can show the URL instead
            }
        }

        // ------------------------------------------------------------- helpers

        internal static TailscaleException MapFunnelError(string stderr, string stdout, string fallback)
        {
            string text = ((stderr ?? string.Empty) + "\n" + (stdout ?? string.Empty)).Trim();
            string lower = text.ToLowerInvariant();
            string first = FirstLine(text);

            if (lower.Contains("https") && (lower.Contains("cert") || lower.Contains("not enabled") || lower.Contains("disabled")))
                return new TailscaleException(TailscaleIssue.HttpsCertificate,
                    "HTTPS certificates are not enabled for your tailnet. Enable HTTPS in the Tailscale admin console.");
            if (lower.Contains("funnel") && (lower.Contains("not enabled") || lower.Contains("policy") || lower.Contains("tailnet")))
                return new TailscaleException(TailscaleIssue.FunnelPolicy,
                    "Funnel is not enabled for your tailnet. Enable it in the Tailscale admin console, then retry.");
            if (lower.Contains("already") || lower.Contains("in use") || lower.Contains("listener"))
                return new TailscaleException(TailscaleIssue.FunnelConflict,
                    first ?? "Another service already uses Funnel on port 443.");
            if (lower.Contains("login") || lower.Contains("logged out") || lower.Contains("auth"))
                return new TailscaleException(TailscaleIssue.AuthRequired,
                    first ?? "Tailscale needs you to sign in again.");

            return new TailscaleException(TailscaleIssue.CliError,
                first ?? (!string.IsNullOrEmpty(fallback) ? fallback : "Tailscale Funnel command failed."));
        }

        private static string FirstLine(string text)
        {
            if (string.IsNullOrWhiteSpace(text)) return null;
            string[] lines = text.Replace("\r\n", "\n").Split('\n');
            foreach (string line in lines)
            {
                string trimmed = line.Trim();
                if (trimmed.Length > 0) return trimmed;
            }
            return null;
        }

        private static string NormalizeHost(string host)
        {
            if (string.IsNullOrEmpty(host)) return null;
            return host.Trim().TrimEnd('.');
        }

        private static string GetString(Dictionary<string, object> dict, string key)
        {
            object value;
            if (dict != null && dict.TryGetValue(key, out value) && value != null)
                return value as string ?? value.ToString();
            return null;
        }

        private static bool GetBool(Dictionary<string, object> dict, string key)
        {
            object value;
            if (dict != null && dict.TryGetValue(key, out value) && value is bool)
                return (bool)value;
            return false;
        }

        private static Dictionary<string, object> GetDictionary(Dictionary<string, object> dict, string key)
        {
            object value;
            if (dict != null && dict.TryGetValue(key, out value))
                return value as Dictionary<string, object>;
            return null;
        }

        private static object[] GetArray(Dictionary<string, object> dict, string key)
        {
            object value;
            if (dict != null && dict.TryGetValue(key, out value))
                return value as object[];
            return null;
        }
    }
}
