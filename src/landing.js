'use strict';

/**
 * Self-hosted install page.
 *
 * Design tokens come from the ui-ux-pro-max "Dark Mode (OLED)" + entertainment
 * palette: deep #0F0F23 canvas, indigo structure, single green CTA.
 */

function escapeHtml(value) {
    return String(value)
        .replace(/&/g, '&amp;')
        .replace(/</g, '&lt;')
        .replace(/>/g, '&gt;')
        .replace(/"/g, '&quot;')
        .replace(/'/g, '&#39;');
}

const ICONS = {
    play: '<svg viewBox="0 0 24 24" fill="none" aria-hidden="true"><path d="M8 5.5v13l11-6.5-11-6.5Z" fill="currentColor"/></svg>',
    copy: '<svg viewBox="0 0 24 24" fill="none" aria-hidden="true"><rect x="9" y="9" width="11" height="11" rx="2.5" stroke="currentColor" stroke-width="1.7"/><path d="M15 6.5A2.5 2.5 0 0 0 12.5 4H6.5A2.5 2.5 0 0 0 4 6.5v6A2.5 2.5 0 0 0 6.5 15" stroke="currentColor" stroke-width="1.7" stroke-linecap="round"/></svg>',
    check: '<svg viewBox="0 0 24 24" fill="none" aria-hidden="true"><path d="m5 12.5 4.5 4.5L19 7" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></svg>',
    external: '<svg viewBox="0 0 24 24" fill="none" aria-hidden="true"><path d="M14 5h5v5M19 5l-7.5 7.5" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"/><path d="M18 14.5v3A2.5 2.5 0 0 1 15.5 20h-8A2.5 2.5 0 0 1 5 17.5v-8A2.5 2.5 0 0 1 7.5 7h3" stroke="currentColor" stroke-width="1.7" stroke-linecap="round"/></svg>',
};

function providerRows(providers) {
    return providers
        .map((provider) => {
            const state = provider.online ? 'online' : 'offline';
            const label = provider.online ? 'Online' : 'Unreachable';
            return `<li class="provider provider--${state}">
        <span class="dot" aria-hidden="true"></span>
        <span class="provider__name">${escapeHtml(provider.name)}</span>
        <span class="provider__meta">${escapeHtml(provider.host)}</span>
        <span class="provider__state">${label}</span>
      </li>`;
        })
        .join('\n');
}

function renderLanding({ manifestUrl, providerStatus, streamMode, version }) {
    const stremioDeepLink = `stremio://${manifestUrl.replace(/^https?:\/\//, '')}`;
    const stremioWebUrl = `https://web.stremio.com/#/addons?addon=${encodeURIComponent(manifestUrl)}`;
    const online = providerStatus.filter((p) => p.online).length;

    return `<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="color-scheme" content="dark">
<title>DhakaFlix Relay — install for Stremio</title>
<meta name="description" content="Install the DhakaFlix (BDIX) Stremio addon and stream from anywhere through the relay.">
<link rel="preconnect" href="https://fonts.googleapis.com">
<link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
<link href="https://fonts.googleapis.com/css2?family=Inter:wght@300;400;500;600;700&display=swap" rel="stylesheet">
<link rel="icon" href="data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 32 32'%3E%3Crect width='32' height='32' rx='8' fill='%234338CA'/%3E%3Cpath d='M12 9.5v13l11-6.5-11-6.5Z' fill='%2322C55E'/%3E%3C/svg%3E">
<style>
  :root {
    --bg: #0F0F23;
    --card: #1B1B30;
    --card-2: #15152B;
    --border: #312E81;
    --fg: #F8FAFC;
    --muted: #A5B0C2;
    --primary: #1E1B4B;
    --secondary: #4338CA;
    --accent: #22C55E;
    --accent-fg: #0F172A;
    --danger: #EF4444;
    --radius: 16px;
    --radius-sm: 10px;
    --font: 'Inter', -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Helvetica, Arial, sans-serif;
    --mono: ui-monospace, SFMono-Regular, 'SF Mono', Menlo, Consolas, monospace;
  }

  * { box-sizing: border-box; }

  html { -webkit-text-size-adjust: 100%; }

  body {
    margin: 0;
    min-height: 100vh;
    background:
      radial-gradient(900px 520px at 18% -10%, rgba(67, 56, 202, 0.38), transparent 62%),
      radial-gradient(760px 460px at 92% 4%, rgba(34, 197, 94, 0.14), transparent 60%),
      var(--bg);
    color: var(--fg);
    font-family: var(--font);
    font-size: 16px;
    line-height: 1.6;
    -webkit-font-smoothing: antialiased;
  }

  .wrap {
    width: min(980px, 100% - 40px);
    margin-inline: auto;
    padding-block: 40px 64px;
  }

  /* ---------- header ---------- */
  .topbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    flex-wrap: wrap;
    margin-bottom: 56px;
  }

  .brand {
    display: flex;
    align-items: center;
    gap: 12px;
    font-weight: 600;
    letter-spacing: -0.01em;
  }

  .brand__mark {
    display: grid;
    place-items: center;
    width: 38px;
    height: 38px;
    border-radius: 11px;
    background: linear-gradient(150deg, var(--secondary), #6D28D9);
    box-shadow: 0 6px 22px rgba(67, 56, 202, 0.45);
    color: var(--accent);
  }

  .brand__mark svg { width: 20px; height: 20px; }

  .brand__sub {
    display: block;
    font-size: 12px;
    font-weight: 400;
    color: var(--muted);
    letter-spacing: 0.01em;
  }

  .chip {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    padding: 6px 12px;
    border: 1px solid var(--border);
    border-radius: 999px;
    background: rgba(27, 27, 48, 0.7);
    font-size: 12.5px;
    color: var(--muted);
    white-space: nowrap;
  }

  .dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--muted);
    flex: 0 0 auto;
  }

  .provider--online .dot { background: var(--accent); box-shadow: 0 0 0 3px rgba(34, 197, 94, 0.16); }
  .provider--offline .dot { background: var(--danger); box-shadow: 0 0 0 3px rgba(239, 68, 68, 0.14); }

  /* ---------- hero ---------- */
  .hero { max-width: 660px; }

  .eyebrow {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    font-size: 12px;
    font-weight: 600;
    letter-spacing: 0.12em;
    text-transform: uppercase;
    color: var(--accent);
    margin-bottom: 18px;
  }

  .eyebrow::before {
    content: "";
    width: 22px;
    height: 1px;
    background: currentColor;
    opacity: 0.7;
  }

  h1 {
    margin: 0 0 18px;
    font-size: clamp(2rem, 5.6vw, 3.15rem);
    line-height: 1.1;
    letter-spacing: -0.032em;
    font-weight: 700;
    text-wrap: balance;
  }

  h1 em {
    font-style: normal;
    background: linear-gradient(92deg, #34D399, #A7F3D0 55%, #818CF8);
    -webkit-background-clip: text;
    background-clip: text;
    color: transparent;
  }

  .lede {
    margin: 0 0 34px;
    font-size: clamp(1rem, 2.4vw, 1.1rem);
    color: var(--muted);
    max-width: 58ch;
  }

  /* ---------- install card ---------- */
  .card {
    background: linear-gradient(180deg, rgba(27, 27, 48, 0.92), rgba(21, 21, 43, 0.92));
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 26px;
    backdrop-filter: blur(6px);
  }

  .install { margin-bottom: 18px; }

  .install__label {
    display: block;
    font-size: 12px;
    font-weight: 500;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--muted);
    margin-bottom: 10px;
  }

  .field {
    display: flex;
    gap: 10px;
    flex-wrap: wrap;
  }

  .url {
    flex: 1 1 260px;
    min-width: 0;
    display: flex;
    align-items: center;
    padding: 13px 15px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--border);
    background: #0C0C1C;
    font-family: var(--mono);
    font-size: 13px;
    color: #C7D2FE;
    overflow-x: auto;
    white-space: nowrap;
    scrollbar-width: thin;
  }

  .btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 9px;
    padding: 13px 20px;
    border-radius: var(--radius-sm);
    border: 1px solid transparent;
    font: inherit;
    font-size: 14.5px;
    font-weight: 600;
    letter-spacing: -0.005em;
    cursor: pointer;
    text-decoration: none;
    transition: transform 120ms ease, background-color 160ms ease, border-color 160ms ease, box-shadow 160ms ease;
  }

  .btn svg { width: 17px; height: 17px; flex: 0 0 auto; }

  .btn--primary {
    background: var(--accent);
    color: var(--accent-fg);
    box-shadow: 0 8px 24px rgba(34, 197, 94, 0.26);
  }

  .btn--primary:hover { background: #16A34A; box-shadow: 0 10px 28px rgba(34, 197, 94, 0.34); }

  .btn--primary:active { transform: translateY(1px); }

  .btn--ghost {
    background: rgba(67, 56, 202, 0.14);
    border-color: var(--border);
    color: var(--fg);
  }

  .btn--ghost:hover { background: rgba(67, 56, 202, 0.28); border-color: #4F46E5; }

  .btn:focus-visible,
  .url:focus-visible,
  a:focus-visible {
    outline: 2px solid #A5B4FC;
    outline-offset: 2px;
  }

  .actions {
    display: flex;
    gap: 10px;
    flex-wrap: wrap;
    margin-top: 18px;
  }

  .hint {
    margin: 14px 0 0;
    font-size: 13px;
    color: var(--muted);
  }

  .hint a { color: #A5B4FC; text-underline-offset: 3px; }

  .copied {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: var(--accent);
    font-size: 13px;
    opacity: 0;
    transition: opacity 160ms ease;
  }

  .copied[data-visible="true"] { opacity: 1; }

  /* ---------- steps ---------- */
  .steps {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(210px, 1fr));
    gap: 14px;
    margin: 22px 0 0;
    padding: 0;
    list-style: none;
    counter-reset: step;
  }

  .steps li {
    counter-increment: step;
    position: relative;
    padding: 20px 20px 20px 20px;
    border: 1px solid rgba(49, 46, 129, 0.7);
    border-radius: var(--radius-sm);
    background: rgba(12, 12, 28, 0.55);
    font-size: 14px;
    color: var(--muted);
  }

  .steps li::before {
    content: counter(step, decimal-leading-zero);
    display: block;
    font-family: var(--mono);
    font-size: 12px;
    font-weight: 600;
    color: var(--accent);
    margin-bottom: 8px;
    letter-spacing: 0.06em;
  }

  .steps strong {
    display: block;
    color: var(--fg);
    font-weight: 600;
    font-size: 14.5px;
    margin-bottom: 3px;
  }

  /* ---------- providers ---------- */
  .section-title {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 12px;
    flex-wrap: wrap;
    margin: 40px 0 14px;
  }

  h2 {
    margin: 0;
    font-size: 13px;
    font-weight: 600;
    letter-spacing: 0.1em;
    text-transform: uppercase;
    color: var(--muted);
  }

  .providers {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 8px;
  }

  .provider {
    display: grid;
    grid-template-columns: auto 1fr auto auto;
    align-items: center;
    gap: 14px;
    padding: 13px 16px;
    border: 1px solid rgba(49, 46, 129, 0.55);
    border-radius: var(--radius-sm);
    background: rgba(27, 27, 48, 0.5);
    font-size: 14px;
  }

  .provider__meta {
    font-family: var(--mono);
    font-size: 12px;
    color: var(--muted);
  }

  .provider__state { font-size: 12.5px; color: var(--muted); }
  .provider--online .provider__state { color: var(--accent); }
  .provider--offline .provider__state { color: var(--destructive, #F87171); }

  .provider--offline { opacity: 0.72; }

  /* ---------- notes ---------- */
  .note {
    margin-top: 32px;
    padding: 18px 20px;
    border-left: 2px solid var(--accent);
    border-radius: 0 var(--radius-sm) var(--radius-sm) 0;
    background: rgba(34, 197, 94, 0.06);
    font-size: 13.5px;
    color: #CBD5E1;
  }

  .note strong { color: var(--fg); }

  footer {
    margin-top: 44px;
    padding-top: 20px;
    border-top: 1px solid rgba(49, 46, 129, 0.5);
    display: flex;
    justify-content: space-between;
    gap: 12px;
    flex-wrap: wrap;
    font-size: 12.5px;
    color: var(--muted);
  }

  @media (max-width: 560px) {
    .wrap { padding-block: 26px 48px; }
    .topbar { margin-bottom: 34px; }
    .card { padding: 20px 18px; }
    .btn { flex: 1 1 auto; }
    .provider { grid-template-columns: auto 1fr auto; }
    .provider__meta { display: none; }
  }

  @media (prefers-reduced-motion: reduce) {
    *, *::before, *::after {
      animation-duration: 0.001ms !important;
      transition-duration: 0.001ms !important;
    }
  }
</style>
</head>
<body>
  <main class="wrap">
    <header class="topbar">
      <div class="brand">
        <span class="brand__mark">${ICONS.play}</span>
        <span>DhakaFlix Relay<span class="brand__sub">Stremio addon · v${escapeHtml(version)}</span></span>
      </div>
      <span class="chip"><span class="dot" aria-hidden="true"></span>${online}/${providerStatus.length} mirrors online</span>
    </header>

    <section class="hero">
      <p class="eyebrow">BDIX · relayed</p>
      <h1>Your DhakaFlix library, <em>anywhere</em>.</h1>
      <p class="lede">
        The BDIX mirrors only answer inside your ISP's network. This relay sits on that
        network and forwards the bytes, so a Stremio install works on any device, on any
        connection — with full seek and resume support.
      </p>
    </section>

    <section class="card install" aria-labelledby="install-label">
      <label class="install__label" id="install-label" for="manifest-url">Manifest URL</label>
      <div class="field">
        <output class="url" id="manifest-url" tabindex="0">${escapeHtml(manifestUrl)}</output>
        <button class="btn btn--ghost" type="button" id="copy-btn" data-url="${escapeHtml(manifestUrl)}">
          <span id="copy-icon">${ICONS.copy}</span><span id="copy-text">Copy</span>
        </button>
      </div>

      <div class="actions">
        <a class="btn btn--primary" href="${escapeHtml(stremioDeepLink)}" rel="noopener">
          ${ICONS.play} Install in Stremio
        </a>
        <a class="btn btn--ghost" href="${escapeHtml(stremioWebUrl)}" target="_blank" rel="noopener noreferrer">
          ${ICONS.external} Open Stremio Web
        </a>
      </div>

      <p class="hint">
        Nothing happened? Install manually: Stremio → <strong>Addons</strong> → <strong>Community</strong> →
        paste the URL above. <span class="copied" id="copied" role="status" aria-live="polite">${ICONS.check} Copied</span>
      </p>
    </section>

    <ol class="steps">
      <li><strong>Install the addon</strong>using the button above, on any device.</li>
      <li><strong>Open any title</strong>whose episode or movie exists on a mirror.</li>
      <li><strong>Press play</strong>and the relay streams it from the BDIX mirror.</li>
    </ol>

    <section aria-labelledby="mirrors-title">
      <div class="section-title">
        <h2 id="mirrors-title">Mirror health</h2>
        <span class="chip">delivery: ${escapeHtml(streamMode)}</span>
      </div>
      <ul class="providers">
        ${providerRows(providerStatus)}
      </ul>
    </section>

    <p class="note">
      <strong>Heads up:</strong> this addon indexes content from private BDIX mirrors and relays it.
      Only stream material you are entitled to access. Playback bandwidth is limited by this
      relay's upstream link, so <code>${escapeHtml(String(streamMode))}</code> delivery is used to keep
      streams playable outside the ISP.
    </p>

    <footer>
      <span>Relay running on Node ${escapeHtml(process.version)}</span>
      <a href="/health" style="color: inherit;">Health status</a>
    </footer>
  </main>

<script>
(function () {
  var btn = document.getElementById('copy-btn');
  var text = document.getElementById('copy-text');
  var icon = document.getElementById('copy-icon');
  var flag = document.getElementById('copied');
  if (!btn) return;

  var check = ${JSON.stringify(ICONS.check)};
  var copyIcon = ${JSON.stringify(ICONS.copy)};

  function done() {
    text.textContent = 'Copied';
    icon.innerHTML = check;
    if (flag) flag.setAttribute('data-visible', 'true');
    setTimeout(function () {
      text.textContent = 'Copy';
      icon.innerHTML = copyIcon;
      if (flag) flag.setAttribute('data-visible', 'false');
    }, 1800);
  }

  btn.addEventListener('click', function () {
    var value = btn.getAttribute('data-url') || '';
    if (navigator.clipboard && window.isSecureContext) {
      navigator.clipboard.writeText(value).then(done, fallback);
    } else {
      fallback();
    }
    function fallback() {
      var area = document.createElement('textarea');
      area.value = value;
      area.setAttribute('readonly', '');
      area.style.position = 'fixed';
      area.style.opacity = '0';
      document.body.appendChild(area);
      area.select();
      try { document.execCommand('copy'); done(); } catch (e) { text.textContent = 'Select the URL above'; }
      document.body.removeChild(area);
    }
  });
})();
</script>
</body>
</html>`;
}

module.exports = { renderLanding };
