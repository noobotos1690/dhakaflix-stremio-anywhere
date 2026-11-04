using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Net;
using System.Net.Sockets;
using System.Runtime.InteropServices;
using System.Text;
using System.Text.RegularExpressions;
using System.Threading;
using System.Threading.Tasks;
using System.Web.Script.Serialization;

namespace DhakaFlixRelay
{
    internal enum RelayState
    {
        Stopped,
        Starting,
        Running,
        Stopping,
        Error
    }

    internal sealed class RelayStateEventArgs : EventArgs
    {
        public RelayState State { get; private set; }
        public string Detail { get; private set; }

        public RelayStateEventArgs(RelayState state, string detail)
        {
            State = state;
            Detail = detail;
        }
    }

    internal sealed class HealthSnapshot
    {
        public int ProvidersOnline;
        public int ProvidersKnown;
        public int UptimeSeconds;
        public string Version;
    }

    /// <summary>
    /// Owns the node relay process: spawns it, streams its output, finds the
    /// public tunnel URL and makes sure the whole tree dies on stop.
    /// </summary>
    internal sealed class RelayController : IDisposable
    {
        private static readonly Regex AnsiPattern = new Regex(@"\x1B\[[0-9;?]*[a-zA-Z]|\x1B\][^\x07]*\x07", RegexOptions.Compiled);
        private static readonly Regex QuickTunnelPattern = new Regex(@"https://[a-z0-9][a-z0-9-]*\.trycloudflare\.com", RegexOptions.IgnoreCase | RegexOptions.Compiled);
        private static readonly Regex PublicUrlPattern = new Regex(@"Public URL:\s*(https?://\S+)", RegexOptions.IgnoreCase | RegexOptions.Compiled);
        private static readonly Regex InstallLinePattern = new Regex(@"^\s*install\s+(https?://\S+)", RegexOptions.IgnoreCase | RegexOptions.Compiled);
        private static readonly Regex ListeningPattern = new Regex(@"Listening on\s+([0-9.]+):(\d+)", RegexOptions.IgnoreCase | RegexOptions.Compiled);
        private static readonly Regex ErrorPattern = new Regex(@"\b(error|failed|fatal|EADDRINUSE|ECONNREFUSED)\b", RegexOptions.IgnoreCase | RegexOptions.Compiled);

        private readonly AppPaths _paths;
        private readonly AppSettings _settings;
        private readonly Action<string, LogLevel> _log;
        private readonly StringBuilder _recent = new StringBuilder();
        private readonly object _sync = new object();

        private Process _process;
        private IntPtr _job;
        private bool _intentionalStop;
        private DateTime _startedAt;
        private System.Threading.Timer _healthTimer;

        public RelayController(AppPaths paths, AppSettings settings, Action<string, LogLevel> log)
        {
            _paths = paths;
            _settings = settings;
            _log = log;
            State = RelayState.Stopped;
            LocalUrl = "http://127.0.0.1:" + settings.Port;
            if (_log != null) _log("Desktop app ready. Relay source: " + paths.Root, LogLevel.Info);
        }

        public event EventHandler<RelayStateEventArgs> StateChanged;
        public event Action<LogEntry> Log;
        public event Action<string> PublicUrlChanged;
        public event EventHandler<HealthSnapshot> HealthUpdated;

        public RelayState State { get; private set; }
        public string PublicUrl { get; private set; }
        public string ManifestUrl { get; private set; }
        public string DeepLink { get; private set; }
        public string LocalUrl { get; private set; }
        public string LastError { get; private set; }

        public bool IsActive
        {
            get { return State == RelayState.Starting || State == RelayState.Running || State == RelayState.Stopping; }
        }

        public void UpdateLocalUrl(int port)
        {
            LocalUrl = "http://127.0.0.1:" + port;
        }

        public TimeSpan Uptime
        {
            get { return _startedAt == default(DateTime) ? TimeSpan.Zero : DateTime.Now - _startedAt; }
        }

        // ------------------------------------------------------------------ start

        public Task StartAsync()
        {
            if (IsActive) return Task.FromResult(true);

            if (_process != null)
            {
                try { _process.Dispose(); } catch { }
                _process = null;
            }

            string node = ResolveNodeForRun();
            if (node == null)
                throw new InvalidOperationException("Node.js is not ready. Run setup first.");

            if (!IsPortFree(_settings.Port))
                throw new InvalidOperationException("Port " + _settings.Port + " is already in use. Stop the other relay or change the port in Preferences.");

            LocalUrl = "http://127.0.0.1:" + _settings.Port;
            PublicUrl = ReadConfiguredPublicUrl();
            ManifestUrl = PublicUrl != null ? PublicUrl.TrimEnd('/') + "/manifest.json" : null;
            DeepLink = ManifestUrl != null ? ToDeepLink(ManifestUrl) : null;
            LastError = null;
            _intentionalStop = false;
            _startedAt = DateTime.Now;
            lock (_sync) _recent.Length = 0;

            var startInfo = new ProcessStartInfo
            {
                FileName = node,
                Arguments = "index.js --tunnel",
                WorkingDirectory = _paths.Root,
                UseShellExecute = false,
                CreateNoWindow = true,
                RedirectStandardOutput = true,
                RedirectStandardError = true,
                StandardOutputEncoding = new UTF8Encoding(false),
                StandardErrorEncoding = new UTF8Encoding(false),
                WindowStyle = ProcessWindowStyle.Hidden,
            };
            startInfo.EnvironmentVariables["PORT"] = _settings.Port.ToString();
            startInfo.EnvironmentVariables["LOG_LEVEL"] = "info";
            startInfo.EnvironmentVariables["PATH"] = ProcessRunner.PrependToPath(Path.GetDirectoryName(node));

            _process = new Process { StartInfo = startInfo, EnableRaisingEvents = true };
            _process.OutputDataReceived += (s, e) => HandleOutput(e.Data, false);
            _process.ErrorDataReceived += (s, e) => HandleOutput(e.Data, true);
            _process.Exited += (s, e) => HandleExit();

            SetState(RelayState.Starting, "Boot sequence started on port " + _settings.Port + ".");
            WriteLog(LogLevel.Info, "Starting relay: node index.js --tunnel (port " + _settings.Port + ")");

            try
            {
                _process.Start();
                AttachToJob(_process);
                _process.BeginOutputReadLine();
                _process.BeginErrorReadLine();
            }
            catch (Exception ex)
            {
                CleanupJob();
                SetState(RelayState.Error, "Could not launch Node.js: " + ex.Message);
                WriteLog(LogLevel.Error, "Failed to launch relay: " + ex.Message);
                throw;
            }

            _healthTimer = new System.Threading.Timer(PollHealth, null, 1500, 3000);
            if (PublicUrl != null)
                RaisePublicUrl(PublicUrl);

            return Task.FromResult(true);
        }

        private string ResolveNodeForRun()
        {
            var probe = new DependencyManager(_paths, (text, level) => WriteLog(level, text));
            DependencySnapshot snapshot = probe.Inspect();
            return snapshot.NodeOk ? snapshot.NodePath : null;
        }

        public static string ReadEnvValue(string envPath, string key)
        {
            try
            {
                if (!File.Exists(envPath)) return null;
                foreach (string raw in File.ReadAllLines(envPath))
                {
                    string line = raw.Trim();
                    if (line.Length == 0 || line.StartsWith("#", StringComparison.Ordinal)) continue;
                    int index = line.IndexOf('=');
                    if (index <= 0) continue;
                    if (!string.Equals(line.Substring(0, index).Trim(), key, StringComparison.OrdinalIgnoreCase)) continue;
                    return line.Substring(index + 1).Trim().Trim('"', '\'');
                }
            }
            catch
            {
                // treated as not configured
            }
            return null;
        }

        private string ReadConfiguredPublicUrl()
        {
            string configured = ReadEnvValue(_paths.EnvFile, "PUBLIC_URL");
            if (!string.IsNullOrEmpty(configured) && (configured.StartsWith("http://") || configured.StartsWith("https://")))
                return configured.TrimEnd('/');
            return null;
        }

        // ------------------------------------------------------------------- stop

        public async Task StopAsync()
        {
            if (!IsActive && _process == null)
            {
                SetState(RelayState.Stopped, "Relay is not running.");
                return;
            }

            SetState(RelayState.Stopping, "Shutting down relay and tunnel...");
            WriteLog(LogLevel.Info, "Stopping relay");
            _intentionalStop = true;

            StopHealthTimer();

            Process process = _process;
            if (process != null)
            {
                CleanupJob();
                try
                {
                    if (!process.HasExited)
                    {
                        bool exited = false;
                        await Task.Run(() =>
                        {
                            for (int i = 0; i < 60; i++)
                            {
                                try
                                {
                                    if (process.HasExited) { exited = true; return; }
                                }
                                catch { return; }
                                Thread.Sleep(100);
                            }
                        }).ConfigureAwait(true);
                        if (!exited) ProcessRunner.KillTree(process);
                    }
                }
                catch
                {
                    // already gone
                }

                try { process.Dispose(); } catch { }
                if (ReferenceEquals(_process, process)) _process = null;
            }

            _startedAt = default(DateTime);
            PublicUrl = null;
            ManifestUrl = null;
            DeepLink = null;
            SetState(RelayState.Stopped, "Relay is not running.");
            WriteLog(LogLevel.Info, "Relay stopped");
        }

        // ------------------------------------------------------------------ output

        private void HandleOutput(string raw, bool isError)
        {
            if (raw == null) return;
            string line = AnsiPattern.Replace(raw, string.Empty).TrimEnd();
            if (line.Length == 0) return;

            if (line.IndexOf("trycloudflare.com", StringComparison.OrdinalIgnoreCase) >= 0)
            {
                Match quick = QuickTunnelPattern.Match(line);
                if (quick.Success) AdoptPublicUrl(quick.Value, false);
            }

            Match publicMatch = PublicUrlPattern.Match(line);
            if (publicMatch.Success) AdoptPublicUrl(publicMatch.Groups[1].Value.TrimEnd('/'), false);

            Match installMatch = InstallLinePattern.Match(line);
            if (installMatch.Success)
            {
                string manifest = installMatch.Groups[1].Value.TrimEnd('/');
                if (manifest.EndsWith("/manifest.json", StringComparison.OrdinalIgnoreCase))
                {
                    string origin = manifest.Substring(0, manifest.Length - "/manifest.json".Length);
                    if (QuickTunnelPattern.IsMatch(origin) || origin.IndexOf("127.0.0.1", StringComparison.Ordinal) >= 0 ||
                        origin.IndexOf("localhost", StringComparison.OrdinalIgnoreCase) >= 0 ||
                        ReadConfiguredPublicUrl() != null)
                    {
                        AdoptPublicUrl(origin, origin.IndexOf("127.0.0.1", StringComparison.Ordinal) >= 0);
                    }
                }
            }

            Match listen = ListeningPattern.Match(line);
            if (listen.Success && State == RelayState.Starting && !_intentionalStop)
            {
                SetState(RelayState.Starting, "Local relay is up - waiting for the public tunnel.");
            }

            if (isError || line.IndexOf("[ERROR]", StringComparison.OrdinalIgnoreCase) >= 0)
                LastError = line;

            lock (_sync)
            {
                _recent.AppendLine(line);
                if (_recent.Length > 24000) _recent.Remove(0, _recent.Length - 20000);
            }

            LogLevel level = Classify(line, isError);
            WriteLog(level, line);
        }

        private LogLevel Classify(string line, bool isError)
        {
            if (line.IndexOf("[ERROR]", StringComparison.OrdinalIgnoreCase) >= 0) return LogLevel.Error;
            if (line.IndexOf("[WARN]", StringComparison.OrdinalIgnoreCase) >= 0) return LogLevel.Warn;
            if (line.IndexOf("[DEBUG]", StringComparison.OrdinalIgnoreCase) >= 0) return LogLevel.Debug;
            if (isError) return LogLevel.Warn;
            if (line.IndexOf("[HTTP]", StringComparison.OrdinalIgnoreCase) >= 0) return LogLevel.Debug;
            if (line.StartsWith("[", StringComparison.Ordinal)) return LogLevel.Info;
            if (ErrorPattern.IsMatch(line)) return LogLevel.Warn;
            return LogLevel.Debug;
        }

        private void AdoptPublicUrl(string url, bool localOnly)
        {
            if (string.IsNullOrEmpty(url)) return;
            url = url.TrimEnd('/');
            if (url.EndsWith("/manifest.json", StringComparison.OrdinalIgnoreCase))
                url = url.Substring(0, url.Length - "/manifest.json".Length);

            bool changed = !string.Equals(PublicUrl, url, StringComparison.OrdinalIgnoreCase);
            PublicUrl = url;
            ManifestUrl = url + "/manifest.json";
            DeepLink = ToDeepLink(ManifestUrl);

            if (changed)
            {
                WriteLog(LogLevel.Info, "Stremio install link: " + ManifestUrl);
                RaisePublicUrl(ManifestUrl);
            }

            if (!localOnly && State != RelayState.Running)
            {
                SetState(RelayState.Running, "Public link is live.");
            }
            else if (localOnly && State != RelayState.Running)
            {
                SetState(RelayState.Running, "Local link only - the tunnel did not start.");
            }
        }

        public static string ToDeepLink(string manifestUrl)
        {
            if (string.IsNullOrEmpty(manifestUrl)) return null;
            string trimmed = manifestUrl.Trim();
            if (trimmed.StartsWith("https://", StringComparison.OrdinalIgnoreCase))
                return "stremio://" + trimmed.Substring(8);
            if (trimmed.StartsWith("http://", StringComparison.OrdinalIgnoreCase))
                return "stremio://" + trimmed.Substring(7);
            return "stremio://" + trimmed;
        }

        private void HandleExit()
        {
            Process process = _process;
            int exitCode = -1;
            try { if (process != null) exitCode = process.ExitCode; } catch { }

            if (_intentionalStop)
            {
                SetState(RelayState.Stopped, "Relay is not running.");
                return;
            }

            StopHealthTimer();
            CleanupJob();

            string detail = LastError;
            if (string.IsNullOrEmpty(detail))
            {
                lock (_sync)
                {
                    string[] lines = _recent.ToString().Split(new[] { '\r', '\n' }, StringSplitOptions.RemoveEmptyEntries);
                    for (int i = lines.Length - 1; i >= 0; i--)
                    {
                        if (lines[i].IndexOf("[ERROR]", StringComparison.OrdinalIgnoreCase) >= 0 ||
                            ErrorPattern.IsMatch(lines[i]))
                        {
                            detail = lines[i];
                            break;
                        }
                    }
                }
            }
            if (string.IsNullOrEmpty(detail)) detail = "The relay exited unexpectedly (code " + exitCode + ").";

            WriteLog(LogLevel.Error, "Relay exited unexpectedly (code " + exitCode + "): " + detail);
            SetState(RelayState.Error, detail);
        }

        private void WriteLog(LogLevel level, string text)
        {
            Action<LogEntry> handler = Log;
            if (handler != null) handler(new LogEntry(level, text));
        }

        private void SetState(RelayState state, string detail)
        {
            if (State == state && detail == null) return;
            State = state;
            EventHandler<RelayStateEventArgs> handler = StateChanged;
            if (handler != null) handler(this, new RelayStateEventArgs(state, detail));
        }

        private void RaisePublicUrl(string manifest)
        {
            Action<string> handler = PublicUrlChanged;
            if (handler != null) handler(manifest);
        }

        // ------------------------------------------------------------------ health

        private void StopHealthTimer()
        {
            System.Threading.Timer timer = _healthTimer;
            _healthTimer = null;
            if (timer != null)
            {
                try { timer.Dispose(); } catch { }
            }
        }

        private async void PollHealth(object state)
        {
            try
            {
                HealthSnapshot snapshot = await FetchHealthAsync().ConfigureAwait(false);
                if (snapshot != null)
                {
                    EventHandler<HealthSnapshot> handler = HealthUpdated;
                    if (handler != null) handler(this, snapshot);
                }
            }
            catch
            {
                // health information is best-effort
            }
        }

        public async Task<HealthSnapshot> FetchHealthAsync()
        {
            if (!IsActive) return null;
            try
            {
                var request = (HttpWebRequest)WebRequest.Create(LocalUrl + "/health");
                request.Timeout = 2500;
                request.ReadWriteTimeout = 2500;
                request.Proxy = null;
                using (WebResponse response = await request.GetResponseAsync().ConfigureAwait(false))
                using (var reader = new StreamReader(response.GetResponseStream()))
                {
                    string body = await reader.ReadToEndAsync().ConfigureAwait(false);
                    var serializer = new JavaScriptSerializer();
                    var payload = serializer.DeserializeObject(body) as Dictionary<string, object>;
                    if (payload == null) return null;

                    var snapshot = new HealthSnapshot();
                    object uptime;
                    if (payload.TryGetValue("uptimeSeconds", out uptime) && uptime != null)
                        int.TryParse(uptime.ToString(), out snapshot.UptimeSeconds);

                    object version;
                    if (payload.TryGetValue("version", out version)) snapshot.Version = version as string;

                    object providers;
                    if (payload.TryGetValue("providers", out providers))
                    {
                        var list = providers as object[];
                        if (list != null)
                        {
                            snapshot.ProvidersKnown = list.Length;
                            foreach (object item in list)
                            {
                                var provider = item as Dictionary<string, object>;
                                object online;
                                if (provider != null && provider.TryGetValue("online", out online) &&
                                    online is bool && (bool)online)
                                {
                                    snapshot.ProvidersOnline++;
                                }
                            }
                        }
                    }
                    return snapshot;
                }
            }
            catch
            {
                return null;
            }
        }

        public static bool IsPortFree(int port)
        {
            try
            {
                var listener = new TcpListener(IPAddress.Any, port);
                listener.Start();
                listener.Stop();
                return true;
            }
            catch
            {
                return false;
            }
        }

        public string RecentTail(int lines)
        {
            lock (_sync)
            {
                string[] all = _recent.ToString().Split(new[] { '\r', '\n' }, StringSplitOptions.RemoveEmptyEntries);
                int start = Math.Max(0, all.Length - lines);
                var builder = new StringBuilder();
                for (int i = start; i < all.Length; i++) builder.AppendLine(all[i]);
                return builder.ToString();
            }
        }

        // ------------------------------------------------------------------- job

        private void AttachToJob(Process process)
        {
            try
            {
                _job = Native.CreateJobObject(IntPtr.Zero, null);
                if (_job == IntPtr.Zero) return;

                var info = new Native.JOBOBJECT_EXTENDED_LIMIT_INFORMATION();
                info.BasicLimitInformation.LimitFlags = Native.JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
                int length = Marshal.SizeOf(typeof(Native.JOBOBJECT_EXTENDED_LIMIT_INFORMATION));
                IntPtr pointer = Marshal.AllocHGlobal(length);
                try
                {
                    Marshal.StructureToPtr(info, pointer, false);
                    if (!Native.SetInformationJobObject(_job, Native.JobObjectExtendedLimitInformation, pointer, (uint)length))
                    {
                        Native.CloseHandle(_job);
                        _job = IntPtr.Zero;
                        return;
                    }
                }
                finally
                {
                    Marshal.FreeHGlobal(pointer);
                }

                if (!Native.AssignProcessToJobObject(_job, process.Handle))
                {
                    Native.CloseHandle(_job);
                    _job = IntPtr.Zero;
                }
            }
            catch
            {
                // A missing job object only means the tree is cleaned up at stop time instead.
            }
        }

        private void CleanupJob()
        {
            IntPtr job = _job;
            _job = IntPtr.Zero;
            if (job != IntPtr.Zero)
            {
                try { Native.CloseHandle(job); } catch { }
            }
        }

        public void Dispose()
        {
            StopHealthTimer();
            try
            {
                if (_process != null)
                {
                    _intentionalStop = true;
                    CleanupJob();
                    try { if (!_process.HasExited) ProcessRunner.KillTree(_process); } catch { }
                    _process.Dispose();
                    _process = null;
                }
            }
            catch
            {
                // shutting down anyway
            }
        }

        private static class Native
        {
            public const uint JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE = 0x2000;
            public const int JobObjectExtendedLimitInformation = 9;

            [StructLayout(LayoutKind.Sequential)]
            public struct JOBOBJECT_BASIC_LIMIT_INFORMATION
            {
                public long PerProcessUserTimeLimit;
                public long PerJobUserTimeLimit;
                public uint LimitFlags;
                public UIntPtr MinimumWorkingSetSize;
                public UIntPtr MaximumWorkingSetSize;
                public uint ActiveProcessLimit;
                public UIntPtr Affinity;
                public uint PriorityClass;
                public uint SchedulingClass;
            }

            [StructLayout(LayoutKind.Sequential)]
            public struct IO_COUNTERS
            {
                public ulong ReadOperationCount;
                public ulong WriteOperationCount;
                public ulong OtherOperationCount;
                public ulong ReadTransferCount;
                public ulong WriteTransferCount;
                public ulong OtherTransferCount;
            }

            [StructLayout(LayoutKind.Sequential)]
            public struct JOBOBJECT_EXTENDED_LIMIT_INFORMATION
            {
                public JOBOBJECT_BASIC_LIMIT_INFORMATION BasicLimitInformation;
                public IO_COUNTERS IoInfo;
                public UIntPtr ProcessMemoryLimit;
                public UIntPtr JobMemoryLimit;
                public UIntPtr PeakProcessMemoryUsed;
                public UIntPtr PeakJobMemoryUsed;
            }

            [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
            public static extern IntPtr CreateJobObject(IntPtr lpJobAttributes, string lpName);

            [DllImport("kernel32.dll", SetLastError = true)]
            public static extern bool SetInformationJobObject(IntPtr hJob, int infoClass, IntPtr info, uint infoLength);

            [DllImport("kernel32.dll", SetLastError = true)]
            public static extern bool AssignProcessToJobObject(IntPtr hJob, IntPtr hProcess);

            [DllImport("kernel32.dll", SetLastError = true)]
            public static extern bool CloseHandle(IntPtr hObject);
        }
    }
}
