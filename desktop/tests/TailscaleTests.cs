using System;
using System.Collections.Generic;
using System.IO;
using System.Text;

namespace DhakaFlixRelay
{
    /// <summary>
    /// Dependency-free test harness for the Tailscale parser, mode names and
    /// .env editing. Run with desktop\tests\run-tests.ps1.
    /// </summary>
    internal static class TailscaleTests
    {
        private static int _failures;
        private static int _checks;

        private static void Check(string name, bool condition, string detail = "")
        {
            _checks++;
            if (condition)
            {
                Console.WriteLine("  ok   " + name);
                return;
            }
            _failures++;
            Console.WriteLine("  FAIL " + name + (detail.Length > 0 ? " - " + detail : string.Empty));
        }

        private static void Main(string[] args)
        {
            string fixtures = args.Length > 0
                ? args[0]
                : Path.Combine(Environment.CurrentDirectory, "fixtures");

            Console.WriteLine("Tailscale parsing");
            TestStatusRunning(Path.Combine(fixtures, "status-running.json"));
            TestStatusNeedsLogin(Path.Combine(fixtures, "status-needs-login.json"));
            TestFunnel(Path.Combine(fixtures, "funnel-active.json"),
                Path.Combine(fixtures, "funnel-conflict.json"),
                Path.Combine(fixtures, "funnel-off.json"));

            Console.WriteLine("Funnel error mapping");
            Check("policy error", TailscaleManager.MapFunnelError(
                "Funnel is not enabled on your tailnet. Visit the admin console.", "", "").Issue == TailscaleIssue.FunnelPolicy);
            Check("certificate error", TailscaleManager.MapFunnelError(
                "HTTPS certificates are not enabled for this tailnet.", "", "").Issue == TailscaleIssue.HttpsCertificate);
            Check("listener conflict", TailscaleManager.MapFunnelError(
                "port 443 is already in use by another listener", "", "").Issue == TailscaleIssue.FunnelConflict);
            Check("auth error", TailscaleManager.MapFunnelError("you are logged out", "", "").Issue == TailscaleIssue.AuthRequired);

            Console.WriteLine("Access mode parsing");
            Check("null defaults to cloudflare", PublicAccessModes.Parse(null) == PublicAccessMode.Cloudflare);
            Check("unknown defaults to cloudflare", PublicAccessModes.Parse("something") == PublicAccessMode.Cloudflare);
            Check("tailscale parses", PublicAccessModes.Parse("TAILSCALE") == PublicAccessMode.Tailscale);
            Check("cloudflare serializes", PublicAccessMode.Cloudflare.ToStorage() == "cloudflare");
            Check("tailscale serializes", PublicAccessMode.Tailscale.ToStorage() == "tailscale");

            Console.WriteLine("Environment file editing");
            TestEnvEditor();

            Console.WriteLine();
            Console.WriteLine(_failures == 0
                ? "PASS - " + _checks + " checks"
                : "FAIL - " + _failures + " of " + _checks + " checks failed");
            Environment.Exit(_failures == 0 ? 0 : 1);
        }

        private static void TestStatusRunning(string path)
        {
            var status = TailscaleManager.ParseStatusJson(File.ReadAllText(path));
            Check("running: connected", status.Connected);
            Check("running: hostname normalized", status.Hostname == "demo-host.tailnet-test.ts.net", status.Hostname);
            Check("running: certificate domain present", status.CertAvailable);
            Check("running: MagicDNS enabled", status.MagicDnsEnabled);
            Check("running: no login required", !status.NeedsLogin);
            Check("running: backend state", status.BackendState == "Running");
        }

        private static void TestStatusNeedsLogin(string path)
        {
            var status = TailscaleManager.ParseStatusJson(File.ReadAllText(path));
            Check("needs-login: not connected", !status.Connected);
            Check("needs-login: flagged", status.NeedsLogin);
            Check("needs-login: auth URL captured", status.AuthUrl != null && status.AuthUrl.StartsWith("https://"));
            Check("needs-login: no certificate domain", !status.CertAvailable);
        }

        private static void TestFunnel(string activePath, string conflictPath, string offPath)
        {
            var active = TailscaleManager.ParseFunnelJson(File.ReadAllText(activePath));
            Check("funnel active: configured", active.Configured);
            Check("funnel active: enabled", active.Enabled);
            Check("funnel active: hostname", active.Hostname == "demo-host.tailnet-test.ts.net", active.Hostname);
            Check("funnel active: target", active.Target == "http://127.0.0.1:7001", active.Target);
            Check("funnel active: matches 7001", active.Matches(7001));
            Check("funnel active: does not match 7002", !active.Matches(7002));

            var conflict = TailscaleManager.ParseFunnelJson(File.ReadAllText(conflictPath));
            Check("funnel conflict: configured", conflict.Configured);
            Check("funnel conflict: other target", conflict.Target == "http://127.0.0.1:8080", conflict.Target);
            Check("funnel conflict: no match", !conflict.Matches(7001));

            var off = TailscaleManager.ParseFunnelJson(File.ReadAllText(offPath));
            Check("funnel off: not configured", !off.Configured);
            Check("funnel off: not enabled", !off.Enabled);
        }

        private static void TestEnvEditor()
        {
            string folder = Path.Combine(Path.GetTempPath(), "dhakaflix-tests-" + Guid.NewGuid().ToString("N"));
            Directory.CreateDirectory(folder);
            string env = Path.Combine(folder, ".env");
            string secret = new string('a', 64);

            try
            {
                File.WriteAllText(env,
                    "# top comment\r\n" +
                    "PORT=7001\r\n" +
                    "PROXY_SECRET=" + secret + "\r\n" +
                    "TUNNEL=1\r\n" +
                    "# keep this comment\r\n" +
                    "CUSTOM=value\r\n",
                    new UTF8Encoding(false));

                string backup;
                bool changed = EnvFileEditor.SetValues(env, new Dictionary<string, string>
                {
                    { "PUBLIC_URL", "https://demo-host.tailnet-test.ts.net" },
                    { "TUNNEL", "0" },
                    { "TUNNEL_TOKEN", string.Empty },
                }, out backup);

                string text = File.ReadAllText(env);
                Check("env: changed", changed);
                Check("env: backup created", !string.IsNullOrEmpty(backup) && File.Exists(backup));
                Check("env: comment preserved", text.Contains("# top comment") && text.Contains("# keep this comment"));
                Check("env: secret preserved", text.Contains("PROXY_SECRET=" + secret));
                Check("env: unrelated key preserved", text.Contains("CUSTOM=value"));
                Check("env: public URL written", text.Contains("PUBLIC_URL=https://demo-host.tailnet-test.ts.net"));
                Check("env: tunnel disabled", text.Contains("TUNNEL=0"));
                Check("env: token cleared", text.Contains("TUNNEL_TOKEN="));
                Check("env: no duplicate key", CountOccurrences(text, "TUNNEL=") == 1);

                string backup2;
                bool changedAgain = EnvFileEditor.SetValues(env, new Dictionary<string, string>
                {
                    { "PUBLIC_URL", "https://demo-host.tailnet-test.ts.net" },
                    { "TUNNEL", "0" },
                    { "TUNNEL_TOKEN", string.Empty },
                }, out backup2);
                Check("env: idempotent", !changedAgain && string.IsNullOrEmpty(backup2));
                Check("env: single backup", Directory.GetFiles(folder, ".env.bak-*").Length == 1);

                Check("env: read value", EnvFileEditor.ReadValue(env, "PUBLIC_URL") == "https://demo-host.tailnet-test.ts.net");
            }
            finally
            {
                try { Directory.Delete(folder, true); } catch { }
            }
        }

        private static int CountOccurrences(string text, string needle)
        {
            int count = 0;
            int index = 0;
            while ((index = text.IndexOf(needle, index, StringComparison.Ordinal)) >= 0)
            {
                count++;
                index += needle.Length;
            }
            return count;
        }
    }
}
