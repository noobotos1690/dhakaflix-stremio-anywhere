using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.IO.Compression;
using System.Net;
using System.Text;
using System.Text.RegularExpressions;
using System.Threading;
using System.Threading.Tasks;

namespace DhakaFlixRelay
{
    internal sealed class DependencySnapshot
    {
        public bool SourceOk;
        public bool NodeOk;
        public string NodePath;
        public string NodeVersion;
        public bool ModulesOk;
        public bool CloudflaredOk;
        public bool EnvOk;
        public string EnvProblem;

        public bool Ready
        {
            get { return SourceOk && NodeOk && ModulesOk && CloudflaredOk && EnvOk; }
        }

        public List<string> Missing()
        {
            var missing = new List<string>();
            if (!SourceOk) missing.Add("Relay source files");
            if (!NodeOk) missing.Add("Node.js 18+");
            if (!ModulesOk) missing.Add("npm packages");
            if (!CloudflaredOk) missing.Add("cloudflared");
            if (!EnvOk) missing.Add("configuration (.env)");
            return missing;
        }
    }

    /// <summary>
    /// Detects and installs everything the relay needs at runtime: a Node.js
    /// runtime (portable, no admin), the npm packages, cloudflared and .env.
    /// </summary>
    internal sealed class DependencyManager
    {
        private static readonly Regex SecretPattern =
            new Regex(@"^\s*PROXY_SECRET\s*=(.*)$", RegexOptions.Multiline);

        private readonly AppPaths _paths;
        private readonly Action<string, LogLevel> _log;

        public DependencyManager(AppPaths paths, Action<string, LogLevel> log)
        {
            _paths = paths;
            _log = log;
        }

        public string NodePath { get; private set; }
        public string NodeVersion { get; private set; }

        // ---------------------------------------------------------------- detect

        public DependencySnapshot Inspect()
        {
            var snapshot = new DependencySnapshot();
            snapshot.SourceOk = _paths.HasRelaySource();

            ResolveNode();
            snapshot.NodeOk = NodePath != null;
            snapshot.NodePath = NodePath;
            snapshot.NodeVersion = NodeVersion;

            snapshot.ModulesOk = File.Exists(Path.Combine(_paths.NodeModulesDir, "stremio-addon-sdk", "package.json")) &&
                                 Directory.Exists(Path.Combine(_paths.NodeModulesDir, "axios"));

            snapshot.CloudflaredOk = File.Exists(_paths.CloudflaredExe) && new FileInfo(_paths.CloudflaredExe).Length > 1024 * 1024;

            snapshot.EnvOk = ReadSecret() != null;
            if (!snapshot.EnvOk)
                snapshot.EnvProblem = File.Exists(_paths.EnvFile)
                    ? "PROXY_SECRET is missing from .env"
                    : ".env has not been created yet";

            return snapshot;
        }

        private void ResolveNode()
        {
            NodePath = null;
            NodeVersion = null;

            bool forcePortable = string.Equals(
                Environment.GetEnvironmentVariable("DHAKAFLIX_FORCE_PORTABLE_NODE"), "1", StringComparison.Ordinal);

            var candidates = new List<string>();
            candidates.Add(_paths.NodeExe);

            if (!forcePortable)
            {
                string overridePath = Environment.GetEnvironmentVariable("DHAKAFLIX_NODE");
                if (!string.IsNullOrEmpty(overridePath)) candidates.Add(overridePath);
                string onPath = ProcessRunner.Which("node.exe");
                if (!string.IsNullOrEmpty(onPath)) candidates.Add(onPath);
                candidates.Add(Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.ProgramFiles), "nodejs", "node.exe"));
                candidates.Add(Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "Programs", "nodejs", "node.exe"));
                string programFilesX86 = Environment.GetFolderPath(Environment.SpecialFolder.ProgramFilesX86);
                if (!string.IsNullOrEmpty(programFilesX86))
                    candidates.Add(Path.Combine(programFilesX86, "nodejs", "node.exe"));
            }

            foreach (string candidate in candidates)
            {
                try
                {
                    if (string.IsNullOrEmpty(candidate) || !File.Exists(candidate)) continue;
                    string version = ProcessRunner.Capture(candidate, "-v", _paths.Root, 8000).Trim();
                    if (version.Length == 0) continue;
                    int major = ParseMajor(version);
                    if (major < 18) continue;
                    NodePath = candidate;
                    NodeVersion = version.TrimStart('v', 'V');
                    return;
                }
                catch
                {
                    // try the next candidate
                }
            }
        }

        private static int ParseMajor(string version)
        {
            string clean = version.Trim().TrimStart('v', 'V');
            int dot = clean.IndexOf('.');
            string head = dot > 0 ? clean.Substring(0, dot) : clean;
            int major;
            return int.TryParse(head, NumberStyles.Integer, CultureInfo.InvariantCulture, out major) ? major : 0;
        }

        private string ReadSecret()
        {
            try
            {
                if (!File.Exists(_paths.EnvFile)) return null;
                string text = File.ReadAllText(_paths.EnvFile);
                Match match = SecretPattern.Match(text);
                if (!match.Success) return null;
                string value = match.Groups[1].Value.Trim().Trim('"', '\'');
                return value.Length >= 32 ? value : null;
            }
            catch
            {
                return null;
            }
        }

        // ---------------------------------------------------------------- install

        public async Task<bool> InstallAsync(IProgress<SetupProgress> progress, CancellationToken cancellation)
        {
            try
            {
                _log("Starting dependency setup", LogLevel.Info);

                if (!_paths.HasRelaySource())
                    throw new InvalidOperationException("index.js / package.json were not found next to the app. Reinstall the app.");

                DependencySnapshot snapshot = Inspect();

                if (!snapshot.NodeOk)
                {
                    if (!await EnsureNodeAsync(progress, cancellation)) return false;
                    snapshot = Inspect();
                    if (!snapshot.NodeOk) throw new InvalidOperationException("Node.js could not be installed.");
                }

                EnsureEnvFile(progress);
                snapshot = Inspect();
                if (!snapshot.EnvOk) throw new InvalidOperationException(snapshot.EnvProblem ?? "Configuration could not be written.");

                if (!snapshot.ModulesOk)
                {
                    await InstallModulesAsync(progress, cancellation);
                    snapshot = Inspect();
                    if (!snapshot.ModulesOk) throw new InvalidOperationException("npm packages are still missing after install.");
                }

                if (!snapshot.CloudflaredOk)
                {
                    await InstallCloudflaredAsync(progress, cancellation);
                    snapshot = Inspect();
                    if (!snapshot.CloudflaredOk) throw new InvalidOperationException("cloudflared could not be downloaded.");
                }

                progress.Report(new SetupProgress { Step = "Ready", Detail = "All dependencies are installed.", Percent = 100 });
                _log("Setup complete - the relay is ready to start", LogLevel.Info);
                return true;
            }
            catch (OperationCanceledException)
            {
                _log("Setup cancelled", LogLevel.Warn);
                return false;
            }
            catch (Exception ex)
            {
                _log("Setup failed: " + ex.Message, LogLevel.Error);
                progress.Report(new SetupProgress { Step = "Setup failed", Detail = ex.Message, Percent = 0 });
                return false;
            }
        }

        private async Task<bool> EnsureNodeAsync(IProgress<SetupProgress> progress, CancellationToken cancellation)
        {
            progress.Report(new SetupProgress
            {
                Step = "Installing Node.js runtime",
                Detail = "Downloading a private copy - no administrator rights needed.",
                Percent = 4
            });
            _log("Node.js 18+ was not found. Downloading a portable runtime.", LogLevel.Info);

            try
            {
                await DownloadPortableNodeAsync(progress, cancellation);
                ResolveNode();
                if (NodePath == null)
                    throw new InvalidOperationException("The downloaded Node.js runtime did not start correctly.");
                _log("Node.js " + NodeVersion + " is ready", LogLevel.Info);
                return true;
            }
            catch (OperationCanceledException)
            {
                throw;
            }
            catch (Exception ex)
            {
                _log("Portable Node.js install failed: " + ex.Message, LogLevel.Error);
                throw new InvalidOperationException(
                    "Could not download Node.js automatically (" + ex.Message + "). " +
                    "Use \"Install Node.js with administrator rights\" from the app, or install it from nodejs.org.", ex);
            }
        }

        private async Task DownloadPortableNodeAsync(IProgress<SetupProgress> progress, CancellationToken cancellation)
        {
            string[] roots = { "https://nodejs.org/dist/latest-v24.x", "https://nodejs.org/dist/latest-v22.x" };
            string sums = null;
            string distRoot = null;

            foreach (string root in roots)
            {
                try
                {
                    sums = await DownloadTextAsync(root + "/SHASUMS256.txt", cancellation);
                    if (!string.IsNullOrEmpty(sums))
                    {
                        distRoot = root;
                        break;
                    }
                }
                catch
                {
                    // try next channel
                }
            }

            if (string.IsNullOrEmpty(sums) || distRoot == null)
                throw new InvalidOperationException("nodejs.org is unreachable.");

            string zipName = null;
            foreach (string line in sums.Split('\n'))
            {
                string trimmed = line.Trim();
                if (trimmed.EndsWith("node-v", StringComparison.Ordinal)) continue;
                if (trimmed.Contains("-win-x64.zip"))
                {
                    int space = trimmed.IndexOf(' ');
                    zipName = space >= 0 ? trimmed.Substring(space + 1).Trim() : trimmed;
                    if (space >= 0) break;
                }
            }
            if (string.IsNullOrEmpty(zipName))
                throw new InvalidOperationException("No Windows x64 build was listed by nodejs.org.");

            string zipUrl = distRoot + "/" + zipName;
            string tempZip = Path.Combine(Path.GetTempPath(), "dhakaflix-node-" + Guid.NewGuid().ToString("N") + ".zip");
            string extractDir = Path.Combine(Path.GetTempPath(), "dhakaflix-node-" + Guid.NewGuid().ToString("N"));

            try
            {
                await DownloadFileAsync(zipUrl, tempZip, "Downloading Node.js", progress, 6, 42, cancellation);

                progress.Report(new SetupProgress { Step = "Installing Node.js runtime", Detail = "Extracting runtime files.", Percent = 44 });
                Directory.CreateDirectory(extractDir);
                ZipFile.ExtractToDirectory(tempZip, extractDir);

                string[] dirs = Directory.GetDirectories(extractDir);
                if (dirs.Length == 0) throw new InvalidOperationException("The Node.js archive was empty.");
                string source = dirs[0];
                foreach (string dir in dirs)
                {
                    if (dir.IndexOf("win-x64", StringComparison.OrdinalIgnoreCase) >= 0)
                    {
                        source = dir;
                        break;
                    }
                }

                Directory.CreateDirectory(_paths.RuntimeDir);
                if (Directory.Exists(_paths.NodeDir))
                {
                    try { Directory.Delete(_paths.NodeDir, true); }
                    catch { Directory.Move(_paths.NodeDir, _paths.NodeDir + "-old-" + DateTime.Now.Ticks); }
                }
                MoveDirectory(source, _paths.NodeDir);
                progress.Report(new SetupProgress { Step = "Installing Node.js runtime", Detail = "Runtime ready.", Percent = 47 });
            }
            finally
            {
                TryDeleteFile(tempZip);
                TryDeleteDirectory(extractDir);
            }
        }

        private void EnsureEnvFile(IProgress<SetupProgress> progress)
        {
            _paths.EnsureFolders();

            if (ReadSecret() != null)
            {
                progress.Report(new SetupProgress { Step = "Writing configuration", Detail = "Configuration already present.", Percent = 49 });
                _log("Keeping the existing PROXY_SECRET in .env", LogLevel.Debug);
                return;
            }

            progress.Report(new SetupProgress { Step = "Writing configuration", Detail = "Creating .env with a signing secret.", Percent = 49 });
            string secret = GenerateSecret();

            if (!File.Exists(_paths.EnvFile))
            {
                string templatePath = Path.Combine(_paths.Root, ".env.example");
                string template = File.Exists(templatePath) ? File.ReadAllText(templatePath) : string.Empty;
                if (!SecretPattern.IsMatch(template))
                {
                    template = string.IsNullOrWhiteSpace(template)
                        ? "# DhakaFlix relay configuration\nPORT=7001\nHOST=0.0.0.0\nSTREAM_MODE=proxy\nPROXY_SECRET=\n"
                        : template + Environment.NewLine + "PROXY_SECRET=";
                }
                string output = SecretPattern.Replace(template, "PROXY_SECRET=" + secret);
                File.WriteAllText(_paths.EnvFile, output, new UTF8Encoding(false));
                _log("Created .env with a generated PROXY_SECRET", LogLevel.Info);
                return;
            }

            string existing = File.ReadAllText(_paths.EnvFile);
            string updated = SecretPattern.Replace(existing, "PROXY_SECRET=" + secret);
            if (updated == existing)
            {
                if (existing.EndsWith("\n", StringComparison.Ordinal)) updated += "PROXY_SECRET=" + secret + "\n";
                else updated += Environment.NewLine + "PROXY_SECRET=" + secret + Environment.NewLine;
            }
            File.WriteAllText(_paths.EnvFile, updated, new UTF8Encoding(false));
            _log("Repaired PROXY_SECRET in .env", LogLevel.Info);
        }

        private static string GenerateSecret()
        {
            var bytes = new byte[32];
            using (var random = System.Security.Cryptography.RandomNumberGenerator.Create())
            {
                random.GetBytes(bytes);
            }
            var builder = new StringBuilder(64);
            foreach (byte b in bytes) builder.Append(b.ToString("x2", CultureInfo.InvariantCulture));
            return builder.ToString();
        }

        private string FindNpmCli()
        {
            if (NodePath == null) return null;
            string nodeDir = Path.GetDirectoryName(NodePath);
            if (string.IsNullOrEmpty(nodeDir)) return null;

            string cli = Path.Combine(nodeDir, "node_modules", "npm", "bin", "npm-cli.js");
            if (File.Exists(cli)) return cli;

            string npmCmd = Path.Combine(nodeDir, "npm.cmd");
            return File.Exists(npmCmd) ? npmCmd : null;
        }

        private async Task InstallModulesAsync(IProgress<SetupProgress> progress, CancellationToken cancellation)
        {
            progress.Report(new SetupProgress
            {
                Step = "Installing npm packages",
                Detail = "Downloading axios, cheerio and the Stremio SDK.",
                Percent = 52
            });
            _log("Installing npm packages with " + NodeVersion + " (this can take a minute)", LogLevel.Info);

            string npmCli = FindNpmCli();
            if (npmCli == null)
                throw new InvalidOperationException("npm was not found next to Node.js. Run setup again or install Node.js from nodejs.org.");

            string nodeDir = Path.GetDirectoryName(NodePath);
            var env = new Dictionary<string, string>
            {
                { "PATH", ProcessRunner.PrependToPath(nodeDir) },
                { "npm_config_audit", "false" },
                { "npm_config_fund", "false" },
                { "npm_config_update_notifier", "false" },
            };

            string arguments = "\"" + npmCli + "\" install --omit=dev --no-audit --no-fund";
            int exitCode = await Task.Run(() => ProcessRunner.Run(
                NodePath,
                arguments,
                _paths.Root,
                env,
                (line, isError) =>
                {
                    if (!string.IsNullOrWhiteSpace(line))
                        _log(line.TrimEnd(), isError ? LogLevel.Warn : LogLevel.Debug);
                },
                cancellation), cancellation).ConfigureAwait(false);

            if (exitCode != 0)
            {
                _log("npm install failed with exit code " + exitCode, LogLevel.Error);
                throw new InvalidOperationException("npm install failed (exit code " + exitCode + "). Check your connection and try again.");
            }

            progress.Report(new SetupProgress { Step = "Installing npm packages", Detail = "Packages installed.", Percent = 88 });
            _log("npm packages installed", LogLevel.Info);
        }

        private async Task InstallCloudflaredAsync(IProgress<SetupProgress> progress, CancellationToken cancellation)
        {
            progress.Report(new SetupProgress
            {
                Step = "Installing cloudflared",
                Detail = "Downloading the tunnel helper (about 40 MB).",
                Percent = 90
            });
            _log("Downloading cloudflared for the public tunnel", LogLevel.Info);

            string script = Path.Combine(_paths.Root, "scripts", "fetch-cloudflared.js");
            if (!File.Exists(script))
                throw new InvalidOperationException("scripts/fetch-cloudflared.js is missing. Reinstall the app.");

            var env = new Dictionary<string, string>
            {
                { "PATH", ProcessRunner.PrependToPath(Path.GetDirectoryName(NodePath)) },
            };

            int exitCode = await Task.Run(() => ProcessRunner.Run(
                NodePath,
                "\"" + script + "\"",
                _paths.Root,
                env,
                (line, isError) =>
                {
                    if (!string.IsNullOrWhiteSpace(line))
                        _log(line.TrimEnd(), isError ? LogLevel.Warn : LogLevel.Debug);
                },
                cancellation), cancellation).ConfigureAwait(false);

            if (exitCode != 0)
                throw new InvalidOperationException("cloudflared download failed. The relay can still run locally, but Stremio needs the public link.");

            progress.Report(new SetupProgress { Step = "Installing cloudflared", Detail = "Tunnel helper ready.", Percent = 98 });
            _log("cloudflared installed", LogLevel.Info);
        }

        // ---------------------------------------------------------------- network

        private static async Task<string> DownloadTextAsync(string url, CancellationToken cancellation)
        {
            return await Task.Run(() =>
            {
                using (var client = new WebClient())
                {
                    client.Headers[HttpRequestHeader.UserAgent] = "DhakaFlix-Relay-Desktop";
                    cancellation.Register(client.CancelAsync);
                    return client.DownloadString(url);
                }
            }, cancellation).ConfigureAwait(false);
        }

        private async Task DownloadFileAsync(string url, string destination, string label,
            IProgress<SetupProgress> progress, int startPercent, int endPercent, CancellationToken cancellation)
        {
            await Task.Run(() =>
            {
                var request = (HttpWebRequest)WebRequest.Create(url);
                request.UserAgent = "DhakaFlix-Relay-Desktop";
                request.Timeout = 30000;
                request.ReadWriteTimeout = 60000;

                using (WebResponse response = request.GetResponse())
                using (Stream stream = response.GetResponseStream())
                using (var file = new FileStream(destination, FileMode.Create, FileAccess.Write, FileShare.None))
                {
                    long total = response.ContentLength;
                    var buffer = new byte[128 * 1024];
                    long read = 0;
                    int lastPercent = -1;
                    var started = DateTime.UtcNow;
                    int current;

                    while ((current = stream.Read(buffer, 0, buffer.Length)) > 0)
                    {
                        cancellation.ThrowIfCancellationRequested();
                        file.Write(buffer, 0, current);
                        read += current;

                        if (total > 0)
                        {
                            int span = endPercent - startPercent;
                            int percent = startPercent + (int)(span * (read / (double)total));
                            if (percent != lastPercent)
                            {
                                lastPercent = percent;
                                progress.Report(new SetupProgress
                                {
                                    Step = label,
                                    Detail = string.Format(CultureInfo.InvariantCulture, "{0:0.0} MB of {1:0.0} MB", read / 1048576d, total / 1048576d),
                                    Percent = percent
                                });
                            }
                        }
                        else if ((DateTime.UtcNow - started).TotalSeconds > 1)
                        {
                            started = DateTime.UtcNow;
                            progress.Report(new SetupProgress
                            {
                                Step = label,
                                Detail = string.Format(CultureInfo.InvariantCulture, "{0:0.0} MB", read / 1048576d),
                                Percent = startPercent
                            });
                        }
                    }
                }
            }, cancellation).ConfigureAwait(false);
        }

        private static void MoveDirectory(string source, string destination)
        {
            try
            {
                Directory.Move(source, destination);
            }
            catch (IOException)
            {
                // The temp folder and the install folder can live on different volumes.
                try
                {
                    CopyDirectory(source, destination);
                }
                catch
                {
                    TryDeleteDirectory(destination);
                    throw;
                }
                TryDeleteDirectory(source);
            }
        }

        private static void CopyDirectory(string source, string destination)
        {
            Directory.CreateDirectory(destination);
            foreach (string file in Directory.GetFiles(source))
            {
                File.Copy(file, Path.Combine(destination, Path.GetFileName(file)), true);
            }
            foreach (string directory in Directory.GetDirectories(source))
            {
                CopyDirectory(directory, Path.Combine(destination, Path.GetFileName(directory)));
            }
        }

        private static void TryDeleteFile(string path)
        {
            try { if (File.Exists(path)) File.Delete(path); } catch { }
        }

        private static void TryDeleteDirectory(string path)
        {
            try { if (Directory.Exists(path)) Directory.Delete(path, true); } catch { }
        }

        // ---------------------------------------------------------------- admin

        public bool TryInstallNodeSystemWide()
        {
            _log("Requesting administrator rights to install Node.js with winget", LogLevel.Info);
            string command =
                "winget install --id OpenJS.NodeJS.LTS -e --silent --accept-package-agreements --accept-source-agreements; exit $LASTEXITCODE";
            string error;
            bool started = Elevation.StartElevated(
                Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.System), "WindowsPowerShell", "v1.0", "powershell.exe"),
                "-NoProfile -ExecutionPolicy Bypass -Command \"" + command + "\"",
                true,
                out error);

            if (!started)
            {
                _log("Elevation failed: " + error, LogLevel.Error);
                return false;
            }

            ResolveNode();
            return NodePath != null;
        }

        public string GetNodeDownloadHint()
        {
            return "The app downloads a private Node.js runtime automatically. If that is blocked, " +
                   "install Node.js 18+ from nodejs.org or let the app install it with administrator rights.";
        }
    }
}
