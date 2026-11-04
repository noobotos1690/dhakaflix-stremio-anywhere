using System;
using System.Diagnostics;
using System.IO;
using System.Net;
using System.Threading;
using System.Windows.Forms;

namespace DhakaFlixRelay
{
    internal static class Program
    {
        [STAThread]
        private static void Main(string[] args)
        {
            // nodejs.org and github.com no longer negotiate anything below TLS 1.2.
            ServicePointManager.SecurityProtocol |= SecurityProtocolType.Tls11 | SecurityProtocolType.Tls12;

            Application.EnableVisualStyles();
            Application.SetCompatibleTextRenderingDefault(false);
            Application.SetUnhandledExceptionMode(UnhandledExceptionMode.CatchException);
            Application.ThreadException += (s, e) => ReportCrash(e.Exception);
            AppDomain.CurrentDomain.UnhandledException += (s, e) => ReportCrash(e.ExceptionObject as Exception);

            bool autostart = HasFlag(args, "--autostart");
            int waitPid = ParseIntFlag(args, "--wait-pid=");
            string root = ParseStringFlag(args, "--root=");

            if (waitPid > 0) WaitForProcess(waitPid, 8000);

            bool createdNew;
            using (var mutex = new Mutex(true, @"Local\DhakaFlixRelay.Desktop", out createdNew))
            {
                if (!createdNew)
                {
                    SignalExistingInstance();
                    return;
                }

                using (var showEvent = new EventWaitHandle(false, EventResetMode.AutoReset, @"Local\DhakaFlixRelay.Show"))
                {
                    var paths = new AppPaths(root);
                    if (!paths.HasRelaySource())
                    {
                        MessageBox.Show(
                            "The relay files were not found next to the app.\r\n\r\nLooked in:\r\n" + paths.Root +
                            "\r\n\r\nReinstall DhakaFlix Relay to fix this.",
                            "DhakaFlix Relay", MessageBoxButtons.OK, MessageBoxIcon.Error);
                        return;
                    }

                    try
                    {
                        paths.EnsureFolders();
                    }
                    catch
                    {
                        // the app reports what it can still do without the log folder
                    }

                    var form = new MainForm(paths, autostart);
                    var waitThread = new Thread(() =>
                    {
                        while (true)
                        {
                            try
                            {
                                showEvent.WaitOne();
                                if (form.IsDisposed) return;
                                form.SignalShow();
                            }
                            catch
                            {
                                return;
                            }
                        }
                    })
                    {
                        IsBackground = true,
                        Name = "show-signal",
                    };
                    waitThread.Start();

                    Application.Run(form);
                }
            }
        }

        private static void ReportCrash(Exception exception)
        {
            if (exception == null) return;
            try
            {
                string folder = Path.Combine(
                    Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData),
                    "DhakaFlix Relay");
                Directory.CreateDirectory(folder);
                File.AppendAllText(Path.Combine(folder, "crash.log"),
                    "===== " + DateTime.Now.ToString("yyyy-MM-dd HH:mm:ss") + " =====" + Environment.NewLine +
                    exception + Environment.NewLine + Environment.NewLine);
            }
            catch
            {
                // nothing else we can do
            }

            try
            {
                MessageBox.Show(
                    "DhakaFlix Relay hit an unexpected error:" + Environment.NewLine + Environment.NewLine +
                    exception.Message + Environment.NewLine + Environment.NewLine +
                    "Details were written to %APPDATA%\\DhakaFlix Relay\\crash.log.",
                    "DhakaFlix Relay", MessageBoxButtons.OK, MessageBoxIcon.Error);
            }
            catch
            {
                // ignore secondary failures
            }
        }

        private static void SignalExistingInstance()
        {
            try
            {
                using (EventWaitHandle show = EventWaitHandle.OpenExisting(@"Local\DhakaFlixRelay.Show"))
                {
                    show.Set();
                }
            }
            catch
            {
                MessageBox.Show("DhakaFlix Relay is already running. Look for it in the notification area.",
                    "DhakaFlix Relay", MessageBoxButtons.OK, MessageBoxIcon.Information);
            }
        }

        private static void WaitForProcess(int pid, int timeoutMs)
        {
            try
            {
                using (Process process = Process.GetProcessById(pid))
                {
                    process.WaitForExit(timeoutMs);
                }
            }
            catch
            {
                // process already gone
            }
        }

        private static bool HasFlag(string[] args, string flag)
        {
            if (args == null) return false;
            foreach (string arg in args)
            {
                if (string.Equals(arg, flag, StringComparison.OrdinalIgnoreCase)) return true;
            }
            return false;
        }

        private static int ParseIntFlag(string[] args, string prefix)
        {
            if (args == null) return 0;
            foreach (string arg in args)
            {
                if (arg != null && arg.StartsWith(prefix, StringComparison.OrdinalIgnoreCase))
                {
                    int value;
                    if (int.TryParse(arg.Substring(prefix.Length), out value)) return value;
                }
            }
            return 0;
        }

        private static string ParseStringFlag(string[] args, string prefix)
        {
            if (args == null) return null;
            foreach (string arg in args)
            {
                if (arg != null && arg.StartsWith(prefix, StringComparison.OrdinalIgnoreCase))
                    return arg.Substring(prefix.Length).Trim('"');
            }
            return null;
        }
    }
}
