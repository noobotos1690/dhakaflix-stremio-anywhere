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
                return loaded;
            }
            catch
            {
                return defaults;
            }
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
