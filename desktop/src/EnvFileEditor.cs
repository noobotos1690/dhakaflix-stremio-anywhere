using System;
using System.Collections.Generic;
using System.IO;
using System.Text;

namespace DhakaFlixRelay
{
    /// <summary>
    /// Line-based .env editor. Keeps comments and unrelated keys untouched and
    /// writes a timestamped backup before changing an existing file.
    /// </summary>
    internal static class EnvFileEditor
    {
        public static bool SetValues(string path, IDictionary<string, string> values, out string backupPath)
        {
            backupPath = null;
            if (values == null || values.Count == 0) return false;

            string existing = File.Exists(path) ? File.ReadAllText(path) : string.Empty;
            string newline = existing.IndexOf("\r\n", StringComparison.Ordinal) >= 0 ? "\r\n" : "\n";
            string[] lines = existing.Length == 0
                ? new string[0]
                : existing.Replace("\r\n", "\n").Split('\n');

            var remaining = new Dictionary<string, string>(values, StringComparer.OrdinalIgnoreCase);
            var output = new List<string>();

            foreach (string raw in lines)
            {
                string key = ActiveKey(raw);
                if (key != null && remaining.ContainsKey(key))
                {
                    output.Add(key + "=" + (remaining[key] ?? string.Empty));
                    remaining.Remove(key);
                    continue;
                }
                output.Add(raw);
            }

            while (output.Count > 0 && output[output.Count - 1].Length == 0)
                output.RemoveAt(output.Count - 1);

            foreach (var pair in remaining)
                output.Add(pair.Key + "=" + (pair.Value ?? string.Empty));

            string updated = string.Join(newline, output.ToArray()) + newline;
            if (string.Equals(updated, existing, StringComparison.Ordinal)) return false;

            if (File.Exists(path))
            {
                backupPath = path + ".bak-" + DateTime.Now.ToString("yyyyMMdd-HHmmss");
                File.Copy(path, backupPath, true);
            }

            File.WriteAllText(path, updated, new UTF8Encoding(false));
            return true;
        }

        public static string ReadValue(string path, string key)
        {
            try
            {
                if (!File.Exists(path)) return null;
                foreach (string raw in File.ReadAllLines(path))
                {
                    string active = ActiveKey(raw);
                    if (active == null || !string.Equals(active, key, StringComparison.OrdinalIgnoreCase)) continue;
                    int index = raw.IndexOf('=');
                    return index < 0 ? string.Empty : raw.Substring(index + 1).Trim().Trim('"', '\'');
                }
            }
            catch
            {
                // treated as missing
            }
            return null;
        }

        private static string ActiveKey(string line)
        {
            if (string.IsNullOrWhiteSpace(line)) return null;
            string trimmed = line.TrimStart();
            if (trimmed.StartsWith("#", StringComparison.Ordinal)) return null;
            int index = trimmed.IndexOf('=');
            if (index <= 0) return null;
            string key = trimmed.Substring(0, index).Trim();
            if (key.Length == 0 || key.IndexOf(' ') >= 0 || key.IndexOf('\t') >= 0) return null;
            return key;
        }
    }
}
