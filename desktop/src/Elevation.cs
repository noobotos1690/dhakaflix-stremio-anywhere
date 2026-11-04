using System;
using System.Diagnostics;
using System.Security.Principal;

namespace DhakaFlixRelay
{
    /// <summary>UAC helpers, used only for actions that genuinely need admin.</summary>
    internal static class Elevation
    {
        public static bool IsAdministrator()
        {
            try
            {
                using (WindowsIdentity identity = WindowsIdentity.GetCurrent())
                {
                    var principal = new WindowsPrincipal(identity);
                    return principal.IsInRole(WindowsBuiltInRole.Administrator);
                }
            }
            catch
            {
                return false;
            }
        }

        public static bool StartElevated(string fileName, string arguments, bool wait, out string error)
        {
            error = null;
            try
            {
                var startInfo = new ProcessStartInfo
                {
                    FileName = fileName,
                    Arguments = arguments,
                    UseShellExecute = true,
                    Verb = "runas",
                    WorkingDirectory = Environment.CurrentDirectory,
                };
                using (Process process = Process.Start(startInfo))
                {
                    if (process == null)
                    {
                        error = "Windows did not start the elevated process.";
                        return false;
                    }
                    if (wait) process.WaitForExit();
                }
                return true;
            }
            catch (Exception ex)
            {
                error = ex.Message;
                return false;
            }
        }
    }
}
