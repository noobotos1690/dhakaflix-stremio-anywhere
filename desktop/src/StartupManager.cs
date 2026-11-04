using System;
using Microsoft.Win32;

namespace DhakaFlixRelay
{
    /// <summary>Per-user "start with Windows" entry (HKCU Run key, no admin needed).</summary>
    internal static class StartupManager
    {
        private const string RunKeyPath = @"Software\Microsoft\Windows\CurrentVersion\Run";
        private const string ValueName = "DhakaFlix Relay";

        public static bool IsEnabled()
        {
            try
            {
                using (RegistryKey key = Registry.CurrentUser.OpenSubKey(RunKeyPath, false))
                {
                    if (key == null) return false;
                    string value = key.GetValue(ValueName) as string;
                    return !string.IsNullOrEmpty(value);
                }
            }
            catch
            {
                return false;
            }
        }

        public static string CurrentCommand()
        {
            try
            {
                using (RegistryKey key = Registry.CurrentUser.OpenSubKey(RunKeyPath, false))
                {
                    if (key == null) return null;
                    return key.GetValue(ValueName) as string;
                }
            }
            catch
            {
                return null;
            }
        }

        public static bool Enable(string exePath)
        {
            try
            {
                using (RegistryKey key = Registry.CurrentUser.CreateSubKey(RunKeyPath))
                {
                    if (key == null) return false;
                    key.SetValue(ValueName, "\"" + exePath + "\" --autostart", RegistryValueKind.String);
                }
                return true;
            }
            catch
            {
                return false;
            }
        }

        public static bool Disable()
        {
            try
            {
                using (RegistryKey key = Registry.CurrentUser.OpenSubKey(RunKeyPath, true))
                {
                    if (key == null) return true;
                    if (key.GetValue(ValueName) != null) key.DeleteValue(ValueName, false);
                }
                return true;
            }
            catch
            {
                return false;
            }
        }
    }
}
