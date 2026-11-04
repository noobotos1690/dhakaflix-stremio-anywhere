using System;

namespace DhakaFlixRelay
{
    internal enum LogLevel
    {
        Debug = 0,
        Info = 1,
        Warn = 2,
        Error = 3
    }

    internal sealed class LogEntry
    {
        public DateTime Time { get; set; }
        public LogLevel Level { get; set; }
        public string Text { get; set; }

        public LogEntry(LogLevel level, string text)
        {
            Time = DateTime.Now;
            Level = level;
            Text = text ?? string.Empty;
        }
    }

    internal sealed class SetupProgress
    {
        public string Step { get; set; }
        public string Detail { get; set; }
        public int Percent { get; set; }
    }
}
