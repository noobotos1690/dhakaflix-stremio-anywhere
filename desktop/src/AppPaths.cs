using System;
using System.IO;
using System.Reflection;

namespace DhakaFlixRelay
{
    /// <summary>
    /// Where the app, the relay source, its runtime and generated files live.
    /// Everything is kept next to the executable so a per-user install stays
    /// fully writable without administrator rights.
    /// </summary>
    internal sealed class AppPaths
    {
        public string Root { get; private set; }
        public string ExePath { get; private set; }
        public string LogsDir { get; private set; }
        public string ToolsDir { get; private set; }
        public string RuntimeDir { get; private set; }
        public string NodeDir { get; private set; }
        public string NodeExe { get; private set; }
        public string NodeModulesDir { get; private set; }
        public string EnvFile { get; private set; }
        public string CloudflaredExe { get; private set; }
        public string LogFile { get; private set; }
        public string SettingsFile { get; private set; }

        public AppPaths(string rootOverride)
        {
            ExePath = Assembly.GetEntryAssembly() != null
                ? Assembly.GetEntryAssembly().Location
                : System.Diagnostics.Process.GetCurrentProcess().MainModule.FileName;

            string candidate = rootOverride;
            if (string.IsNullOrEmpty(candidate)) candidate = Environment.GetEnvironmentVariable("DHAKAFLIX_ROOT");
            if (string.IsNullOrEmpty(candidate)) candidate = Path.GetDirectoryName(ExePath);
            Root = Path.GetFullPath(candidate);

            if (!File.Exists(Path.Combine(Root, "index.js")))
            {
                // A build-tree launch (exe in desktop\dist) should still find the relay.
                string parent = Path.GetDirectoryName(Root);
                if (!string.IsNullOrEmpty(parent) && File.Exists(Path.Combine(parent, "index.js")))
                    Root = parent;
            }

            LogsDir = Path.Combine(Root, "logs");
            ToolsDir = Path.Combine(Root, "tools");
            RuntimeDir = Path.Combine(Root, "runtime");
            NodeDir = Path.Combine(RuntimeDir, "node");
            NodeExe = Path.Combine(NodeDir, "node.exe");
            NodeModulesDir = Path.Combine(Root, "node_modules");
            EnvFile = Path.Combine(Root, ".env");
            CloudflaredExe = Path.Combine(ToolsDir, "cloudflared.exe");
            LogFile = Path.Combine(LogsDir, "relay-desktop.log");
            SettingsFile = Path.Combine(
                Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData),
                "DhakaFlix Relay", "settings.json");
        }

        public bool HasRelaySource()
        {
            return File.Exists(Path.Combine(Root, "index.js")) &&
                   File.Exists(Path.Combine(Root, "package.json"));
        }

        public void EnsureFolders()
        {
            Directory.CreateDirectory(LogsDir);
            Directory.CreateDirectory(ToolsDir);
        }
    }
}
