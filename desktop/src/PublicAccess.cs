using System;

namespace DhakaFlixRelay
{
    internal enum PublicAccessMode
    {
        Cloudflare,
        Tailscale
    }

    internal static class PublicAccessModes
    {
        public const string CloudflareStorage = "cloudflare";
        public const string TailscaleStorage = "tailscale";

        public static PublicAccessMode Parse(string value)
        {
            return string.Equals(value, TailscaleStorage, StringComparison.OrdinalIgnoreCase)
                ? PublicAccessMode.Tailscale
                : PublicAccessMode.Cloudflare;
        }

        public static string ToStorage(this PublicAccessMode mode)
        {
            return mode == PublicAccessMode.Tailscale ? TailscaleStorage : CloudflareStorage;
        }

        public static string DisplayName(this PublicAccessMode mode)
        {
            return mode == PublicAccessMode.Tailscale ? "Tailscale Funnel" : "Cloudflare Quick Tunnel";
        }

        public static string ShortName(this PublicAccessMode mode)
        {
            return mode == PublicAccessMode.Tailscale ? "Stable Tailscale URL" : "Quick tunnel";
        }
    }
}
