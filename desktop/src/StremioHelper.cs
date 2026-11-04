using System;
using System.Diagnostics;
using System.IO;
using System.Text.RegularExpressions;
using Microsoft.Win32;

namespace DhakaFlixRelay
{
    /// <summary>Finds and launches the Stremio desktop app.</summary>
    internal static class StremioHelper
    {
        private static string _cachedExecutable;

        public static string FindExecutable()
        {
            if (!string.IsNullOrEmpty(_cachedExecutable) && File.Exists(_cachedExecutable))
                return _cachedExecutable;

            string local = Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData);
            string programFiles = Environment.GetFolderPath(Environment.SpecialFolder.ProgramFiles);
            string programFilesX86 = Environment.GetFolderPath(Environment.SpecialFolder.ProgramFilesX86);

            string[] candidates =
            {
                Path.Combine(local, "Programs", "Stremio", "Stremio.exe"),
                Path.Combine(local, "Programs", "stremio", "Stremio.exe"),
                Path.Combine(programFiles, "Stremio", "Stremio.exe"),
                Path.Combine(programFilesX86, "Stremio", "Stremio.exe"),
            };

            foreach (string candidate in candidates)
            {
                try
                {
                    if (File.Exists(candidate))
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

            string fromProtocol = ExecutableFromProtocol();
            if (!string.IsNullOrEmpty(fromProtocol))
            {
                _cachedExecutable = fromProtocol;
                return fromProtocol;
            }

            return null;
        }

        public static bool IsInstalled()
        {
            if (FindExecutable() != null) return true;
            try
            {
                using (RegistryKey key = Registry.ClassesRoot.OpenSubKey("stremio", false))
                {
                    return key != null;
                }
            }
            catch
            {
                return false;
            }
        }

        /// <summary>Opens the addon install prompt (stremio://host/manifest.json).</summary>
        public static bool OpenInstall(string manifestUrl, out string error)
        {
            error = null;
            if (string.IsNullOrEmpty(manifestUrl))
            {
                error = "There is no addon link yet. Start the relay first.";
                return false;
            }

            string deepLink = RelayController.ToDeepLink(manifestUrl);
            try
            {
                Process.Start(new ProcessStartInfo(deepLink) { UseShellExecute = true });
                return true;
            }
            catch (Exception ex)
            {
                error = ex.Message;
            }

            // Fallback: hand the URL to the Stremio executable directly.
            string executable = FindExecutable();
            if (executable != null)
            {
                try
                {
                    Process.Start(new ProcessStartInfo(executable, "\"" + deepLink + "\"") { UseShellExecute = true });
                    error = null;
                    return true;
                }
                catch (Exception ex)
                {
                    error = ex.Message;
                }
            }

            return false;
        }

        /// <summary>Opens Stremio itself, without trying to install anything.</summary>
        public static bool OpenApp(out string error)
        {
            error = null;
            string executable = FindExecutable();
            if (executable != null)
            {
                try
                {
                    Process.Start(new ProcessStartInfo(executable) { UseShellExecute = true });
                    return true;
                }
                catch (Exception ex)
                {
                    error = ex.Message;
                }
            }

            try
            {
                Process.Start(new ProcessStartInfo("stremio://") { UseShellExecute = true });
                return true;
            }
            catch (Exception ex)
            {
                error = ex.Message;
            }

            return false;
        }

        public static void OpenDownloadPage()
        {
            try
            {
                Process.Start(new ProcessStartInfo("https://www.stremio.com/downloads") { UseShellExecute = true });
            }
            catch
            {
                // the caller already showed a message
            }
        }

        private static string ExecutableFromProtocol()
        {
            try
            {
                using (RegistryKey key = Registry.ClassesRoot.OpenSubKey(@"stremio\shell\open\command", false))
                {
                    if (key == null) return null;
                    string command = key.GetValue(null) as string;
                    return ParseExecutable(command);
                }
            }
            catch
            {
                return null;
            }
        }

        private static string ParseExecutable(string command)
        {
            if (string.IsNullOrWhiteSpace(command)) return null;
            Match quoted = Regex.Match(command, "^\"([^\"]+\\.exe)\"", RegexOptions.IgnoreCase);
            if (quoted.Success && File.Exists(quoted.Groups[1].Value))
                return quoted.Groups[1].Value;

            Match bare = Regex.Match(command, @"^([^\s]+\.exe)", RegexOptions.IgnoreCase);
            if (bare.Success && File.Exists(bare.Groups[1].Value))
                return bare.Groups[1].Value;

            return null;
        }
    }
}
