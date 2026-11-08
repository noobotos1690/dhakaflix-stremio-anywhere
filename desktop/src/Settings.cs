using System;
using System.Collections.Generic;
using System.IO;
using System.Web.Script.Serialization;

namespace DhakaFlixRelay
{
    /// <summary>Small JSON-backed preference file in %APPDATA%.</summary>
    internal sealed class AppSettings
    {
        public int Port { get; set; }
        public bool AutoStartRelay { get; set; }
        public bool FollowLogs { get; set; }
        public bool FirstRun { get; set; }
        public bool CloseToTray { get; set; }
        public string PublicAccessMode { get; set; }
        public string TailscaleHostname { get; set; }
        public string CloudflarePublicUrl { get; set; }
        public string CloudflareTunnelToken { get; set; }
        public string CloudflareTunnelValue { get; set; }
        public bool CloudflareConfigCaptured { get; set; }

        private static string PathFor()
        {
            return Path.Combine(
                Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData),
                "DhakaFlix Relay", "settings.json");
        }

        public static AppSettings CreateDefault()
        {
            return new AppSettings
            {
                Port = 7001,
                AutoStartRelay = true,
                FollowLogs = true,
                FirstRun = true,
                CloseToTray = false,
                PublicAccessMode = PublicAccessModes.CloudflareStorage,
                TailscaleHostname = string.Empty,
                CloudflarePublicUrl = string.Empty,
                CloudflareTunnelToken = string.Empty,
                CloudflareTunnelValue = string.Empty,
            };
        }

        public static AppSettings Load()
        {
            AppSettings defaults = CreateDefault();
            try
            {
                string path = PathFor();
                if (!File.Exists(path)) return defaults;

                string text = File.ReadAllText(path);
                var serializer = new JavaScriptSerializer();
                var loaded = serializer.Deserialize<AppSettings>(text);
                if (loaded == null) return defaults;

                if (loaded.Port < 1024 || loaded.Port > 65535) loaded.Port = defaults.Port;
                if (!string.Equals(loaded.PublicAccessMode, PublicAccessModes.TailscaleStorage, StringComparison.OrdinalIgnoreCase))
                    loaded.PublicAccessMode = PublicAccessModes.CloudflareStorage;
                if (loaded.TailscaleHostname == null) loaded.TailscaleHostname = string.Empty;
                if (loaded.CloudflarePublicUrl == null) loaded.CloudflarePublicUrl = string.Empty;
                if (loaded.CloudflareTunnelToken == null) loaded.CloudflareTunnelToken = string.Empty;
                if (loaded.CloudflareTunnelValue == null) loaded.CloudflareTunnelValue = string.Empty;
                return loaded;
            }
            catch
            {
                return defaults;
            }
        }

        public PublicAccessMode GetMode()
        {
            return PublicAccessModes.Parse(PublicAccessMode);
        }

        public void SetMode(PublicAccessMode mode)
        {
            PublicAccessMode = mode.ToStorage();
        }

        public void Save()
        {
            try
            {
                string path = PathFor();
                Directory.CreateDirectory(Path.GetDirectoryName(path));
                var serializer = new JavaScriptSerializer();
                File.WriteAllText(path, serializer.Serialize(this));
            }
            catch
            {
                // Preferences are a convenience; never fail the app over them.
            }
        }
    }
}
