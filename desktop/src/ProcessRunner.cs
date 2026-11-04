using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Text;
using System.Threading;

namespace DhakaFlixRelay
{
    /// <summary>Runs a console tool with live, line-buffered output.</summary>
    internal static class ProcessRunner
    {
        public static int Run(
            string fileName,
            string arguments,
            string workingDirectory,
            IDictionary<string, string> environment,
            Action<string, bool> onLine,
            CancellationToken cancellation)
        {
            var startInfo = new ProcessStartInfo
            {
                FileName = fileName,
                Arguments = arguments,
                WorkingDirectory = workingDirectory,
                UseShellExecute = false,
                CreateNoWindow = true,
                RedirectStandardOutput = true,
                RedirectStandardError = true,
                StandardOutputEncoding = new UTF8Encoding(false),
                StandardErrorEncoding = new UTF8Encoding(false),
                WindowStyle = ProcessWindowStyle.Hidden,
            };

            if (environment != null)
            {
                foreach (var pair in environment)
                {
                    if (pair.Value == null) startInfo.EnvironmentVariables.Remove(pair.Key);
                    else startInfo.EnvironmentVariables[pair.Key] = pair.Value;
                }
            }

            using (var process = new Process { StartInfo = startInfo, EnableRaisingEvents = true })
            {
                process.OutputDataReceived += (s, e) =>
                {
                    if (e.Data != null && onLine != null) onLine(e.Data, false);
                };
                process.ErrorDataReceived += (s, e) =>
                {
                    if (e.Data != null && onLine != null) onLine(e.Data, true);
                };

                process.Start();
                process.BeginOutputReadLine();
                process.BeginErrorReadLine();

                while (!process.WaitForExit(120))
                {
                    if (cancellation.IsCancellationRequested)
                    {
                        KillTree(process);
                        break;
                    }
                }

                try
                {
                    process.WaitForExit();
                }
                catch
                {
                    // process already gone
                }

                return process.HasExited ? process.ExitCode : -1;
            }
        }

        public static void KillTree(Process process)
        {
            if (process == null) return;
            try
            {
                if (process.HasExited) return;
                var startInfo = new ProcessStartInfo("taskkill", "/PID " + process.Id + " /T /F")
                {
                    UseShellExecute = false,
                    CreateNoWindow = true,
                    RedirectStandardOutput = true,
                    RedirectStandardError = true,
                };
                using (var killer = Process.Start(startInfo))
                {
                    if (killer != null)
                    {
                        killer.StandardOutput.ReadToEnd();
                        killer.StandardError.ReadToEnd();
                        killer.WaitForExit(5000);
                    }
                }
            }
            catch
            {
                try { process.Kill(); } catch { }
            }
        }

        public static void KillTree(int processId)
        {
            try
            {
                var startInfo = new ProcessStartInfo("taskkill", "/PID " + processId + " /T /F")
                {
                    UseShellExecute = false,
                    CreateNoWindow = true,
                    RedirectStandardOutput = true,
                    RedirectStandardError = true,
                };
                using (var killer = Process.Start(startInfo))
                {
                    if (killer != null)
                    {
                        killer.StandardOutput.ReadToEnd();
                        killer.StandardError.ReadToEnd();
                        killer.WaitForExit(5000);
                    }
                }
            }
            catch
            {
                // nothing else to try
            }
        }

        /// <summary>Captures both streams and returns stdout.</summary>
        public static string Capture(string fileName, string arguments, string workingDirectory, int timeoutMs)
        {
            try
            {
                var startInfo = new ProcessStartInfo
                {
                    FileName = fileName,
                    Arguments = arguments,
                    WorkingDirectory = workingDirectory,
                    UseShellExecute = false,
                    CreateNoWindow = true,
                    RedirectStandardOutput = true,
                    RedirectStandardError = true,
                    StandardOutputEncoding = new UTF8Encoding(false),
                    StandardErrorEncoding = new UTF8Encoding(false),
                };

                using (var process = Process.Start(startInfo))
                {
                    if (process == null) return string.Empty;
                    string output = process.StandardOutput.ReadToEnd();
                    process.StandardError.ReadToEnd();
                    if (!process.WaitForExit(timeoutMs))
                    {
                        KillTree(process);
                        return string.Empty;
                    }
                    return output;
                }
            }
            catch
            {
                return string.Empty;
            }
        }

        public static string PrependToPath(string directory)
        {
            string path = Environment.GetEnvironmentVariable("PATH") ?? string.Empty;
            if (string.IsNullOrEmpty(directory)) return path;
            return directory.TrimEnd('\\') + ";" + path;
        }

        public static string Which(string executable)
        {
            string path = Environment.GetEnvironmentVariable("PATH") ?? string.Empty;
            foreach (string raw in path.Split(';'))
            {
                string dir = raw.Trim();
                if (dir.Length == 0) continue;
                try
                {
                    string candidate = Path.Combine(dir, executable);
                    if (File.Exists(candidate)) return candidate;
                }
                catch
                {
                    // malformed PATH entry
                }
            }
            return null;
        }
    }
}
