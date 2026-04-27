<script lang="ts">
  import { onMount } from "svelte";
  import en from "../config/i18n/en.json";
  import sk from "../config/i18n/sk.json";
  import settings from "../config/settings.json";
  import type { JsonValue } from "../../../../../sdk/typescript/mesh-core-api/src/index";

  type Dictionary = Record<string, string>;

  const dictionaries: Record<string, Dictionary> = { en, sk };
  const defaultLocale = settings.i18n.default_locale;
  const locale =
    typeof navigator !== "undefined" && navigator.language.toLowerCase().startsWith("sk")
      ? "sk"
      : defaultLocale;
  const messages = dictionaries[locale] ?? dictionaries[defaultLocale] ?? dictionaries.en;

  const subscribedBindables = [
    "audio.muted",
    "audio.percent",
    "power.available",
    "power.level",
    "power.charging",
    "power.time_remaining_minutes",
    "power.time_to_full_minutes",
  ];

  let audioMuted = false;
  let audioPercent = 0;
  let audioTooltip = "Volume unavailable";
  let audioIconName = "audio-volume-muted";

  let powerAvailable = false;
  let powerLevel = 0;
  let powerCharging = false;
  let powerTimeRemainingMinutes = 0;
  let powerTimeToFullMinutes = 0;
  let batteryIconName = "battery-empty";
  let batteryLabel = "N/A";
  let batteryTooltip = "Battery status unavailable";
  let batteryAriaLabel = "Battery status unavailable";

  function t(key: string): string {
    return messages[key] ?? dictionaries.en[key] ?? key;
  }

  function readBindable<T extends JsonValue>(id: string): T | undefined {
    return window.__meshBindableStore__?.get(id) as T | undefined;
  }

  function coerceBoolean(value: JsonValue | undefined, fallback = false): boolean {
    return typeof value === "boolean" ? value : fallback;
  }

  function coerceNumber(value: JsonValue | undefined, fallback = 0): number {
    return typeof value === "number" ? value : fallback;
  }

  function syncAudioState() {
    if (audioMuted || audioPercent <= 0) {
      audioIconName = "audio-volume-muted";
    } else if (audioPercent < 34) {
      audioIconName = "audio-volume-low";
    } else if (audioPercent < 67) {
      audioIconName = "audio-volume-medium";
    } else {
      audioIconName = "audio-volume-high";
    }

    audioTooltip = audioMuted
      ? `Volume muted at ${audioPercent}%`
      : `Volume ${audioPercent}%`;
  }

  function roundPercent(level: number): number {
    return Math.round(Math.min(Math.max(level, 0), 1) * 100);
  }

  function formatMinutes(totalMinutes: number): string {
    if (!totalMinutes || totalMinutes <= 0) {
      return "Unavailable";
    }

    const hours = Math.floor(totalMinutes / 60);
    const minutes = totalMinutes % 60;
    if (hours > 0 && minutes > 0) {
      return `${hours}h ${minutes}m`;
    }
    if (hours > 0) {
      return `${hours}h`;
    }
    return `${minutes}m`;
  }

  function batteryIconFor(percentValue: number): string {
    if (percentValue >= 95) return "battery-full";
    if (percentValue >= 70) return "battery-good";
    if (percentValue >= 45) return "battery-low";
    if (percentValue >= 20) return "battery-caution";
    return "battery-empty";
  }

  function syncPowerState() {
    if (!powerAvailable) {
      batteryIconName = "battery-empty";
      batteryLabel = "N/A";
      batteryTooltip = "Battery status unavailable";
      batteryAriaLabel = "Battery status unavailable";
      return;
    }

    const percentValue = roundPercent(powerLevel);
    const remainingLabel = powerCharging
      ? formatMinutes(powerTimeToFullMinutes)
      : formatMinutes(powerTimeRemainingMinutes);
    const remainingPrefix = powerCharging ? "Time to full" : "Estimated remaining";
    const statusLabel = powerCharging ? "Charging" : "On battery";

    batteryIconName = batteryIconFor(percentValue);
    batteryLabel = `${percentValue}%`;
    batteryTooltip = `${statusLabel} at ${percentValue}%. ${remainingPrefix}: ${remainingLabel}.`;
    batteryAriaLabel = `Battery ${percentValue} percent, ${statusLabel}`;
  }

  function refreshBindableState() {
    audioMuted = coerceBoolean(readBindable("audio.muted"));
    audioPercent = coerceNumber(readBindable("audio.percent"));
    syncAudioState();

    powerAvailable = coerceBoolean(readBindable("power.available"));
    powerLevel = coerceNumber(readBindable("power.level"));
    powerCharging = coerceBoolean(readBindable("power.charging"));
    powerTimeRemainingMinutes = coerceNumber(readBindable("power.time_remaining_minutes"));
    powerTimeToFullMinutes = coerceNumber(readBindable("power.time_to_full_minutes"));
    syncPowerState();
  }

  function shellEvent(channel: string, payload: Record<string, JsonValue>) {
    window.meshHost?.emitEvent(channel, payload);
  }

  function buttonPosition(node: EventTarget | null) {
    if (!(node instanceof HTMLElement)) {
      return { margin_top: 0, margin_left: 0, width: 44 };
    }

    const rect = node.getBoundingClientRect();
    return {
      margin_top: Math.max(0, Math.round(rect.top)),
      margin_left: Math.max(0, Math.round(rect.left)),
      width: Math.max(1, Math.round(rect.width)),
    };
  }

  function onSettingsClick(event: MouseEvent) {
    const position = buttonPosition(event.currentTarget);
    const quickSettingsWidth = 480;
    shellEvent("shell.position-surface", {
      surface_id: "@mesh/quick-settings",
      margin_top: position.margin_top,
      margin_left: Math.max(0, position.margin_left - (quickSettingsWidth - position.width)),
    });
    shellEvent("shell.toggle-surface", { surface_id: "@mesh/quick-settings" });
    shellEvent("shell.hide-surface", { surface_id: "@mesh/volume-bar" });
  }

  function onVolumeClick(event: MouseEvent) {
    const position = buttonPosition(event.currentTarget);
    shellEvent("shell.position-surface", {
      surface_id: "@mesh/volume-bar",
      margin_top: position.margin_top,
      margin_left: position.margin_left,
    });
    shellEvent("shell.toggle-surface", { surface_id: "@mesh/volume-bar" });
    shellEvent("shell.hide-surface", { surface_id: "@mesh/quick-settings" });
  }

  function onBatteryClick(event: MouseEvent) {
    const position = buttonPosition(event.currentTarget);
    shellEvent("shell.position-surface", {
      surface_id: "@mesh/power-details",
      margin_top: position.margin_top,
      margin_left: position.margin_left,
    });
    shellEvent("shell.toggle-surface", { surface_id: "@mesh/power-details" });
    shellEvent("shell.hide-surface", { surface_id: "@mesh/volume-bar" });
  }

  function invokeShellCommand(command: string) {
    window.meshHost?.invokeCore(command);
  }

  function iconSvg(name: string): string {
    switch (name) {
      case "settings":
        return `<svg viewBox="0 0 24 24" aria-hidden="true"><path fill="currentColor" d="M19.14 12.94c.04-.31.06-.63.06-.94s-.02-.63-.07-.94l2.03-1.58a.5.5 0 0 0 .12-.63l-1.92-3.32a.5.5 0 0 0-.6-.22l-2.39.96a7.2 7.2 0 0 0-1.63-.94l-.36-2.54a.5.5 0 0 0-.5-.42h-3.84a.5.5 0 0 0-.5.42l-.36 2.54c-.58.23-1.13.54-1.63.94l-2.39-.96a.5.5 0 0 0-.6.22L2.65 8.85a.5.5 0 0 0 .12.63l2.03 1.58c-.05.31-.08.63-.08.94s.03.63.08.94l-2.03 1.58a.5.5 0 0 0-.12.63l1.92 3.32c.13.22.39.31.6.22l2.39-.96c.5.4 1.05.72 1.63.94l.36 2.54c.04.24.25.42.5.42h3.84c.25 0 .46-.18.5-.42l.36-2.54c.58-.23 1.13-.54 1.63-.94l2.39.96c.22.09.47 0 .6-.22l1.92-3.32a.5.5 0 0 0-.12-.63l-2.02-1.58ZM12 15.5A3.5 3.5 0 1 1 12 8.5a3.5 3.5 0 0 1 0 7Z"/></svg>`;
      case "audio-volume-low":
        return `<svg viewBox="0 0 24 24" aria-hidden="true"><path fill="currentColor" d="M14 3.23v17.54a1 1 0 0 1-1.64.77L7.58 17H4a1 1 0 0 1-1-1V8a1 1 0 0 1 1-1h3.58l4.78-4.54A1 1 0 0 1 14 3.23Zm3.1 5.67a1 1 0 0 1 1.4.12 4.8 4.8 0 0 1 0 5.96 1 1 0 1 1-1.52-1.3 2.8 2.8 0 0 0 0-3.36 1 1 0 0 1 .12-1.42Z"/></svg>`;
      case "audio-volume-medium":
        return `<svg viewBox="0 0 24 24" aria-hidden="true"><path fill="currentColor" d="M14 3.23v17.54a1 1 0 0 1-1.64.77L7.58 17H4a1 1 0 0 1-1-1V8a1 1 0 0 1 1-1h3.58l4.78-4.54A1 1 0 0 1 14 3.23Zm3.75 3.52a1 1 0 0 1 1.4.11 8.04 8.04 0 0 1 0 10.28 1 1 0 1 1-1.51-1.31 6.04 6.04 0 0 0 0-7.66 1 1 0 0 1 .11-1.42Zm-2.5 2.14a1 1 0 0 1 1.4.12 4.8 4.8 0 0 1 0 5.98 1 1 0 0 1-1.52-1.3 2.8 2.8 0 0 0 0-3.38 1 1 0 0 1 .12-1.42Z"/></svg>`;
      case "audio-volume-high":
        return `<svg viewBox="0 0 24 24" aria-hidden="true"><path fill="currentColor" d="M14 3.23v17.54a1 1 0 0 1-1.64.77L7.58 17H4a1 1 0 0 1-1-1V8a1 1 0 0 1 1-1h3.58l4.78-4.54A1 1 0 0 1 14 3.23Zm4.38 1.58a1 1 0 0 1 1.4.1 11.4 11.4 0 0 1 0 14.18 1 1 0 0 1-1.5-1.32 9.4 9.4 0 0 0 0-11.54 1 1 0 0 1 .1-1.42Zm-2.63 2.36a1 1 0 0 1 1.4.11 8.04 8.04 0 0 1 0 10.44 1 1 0 1 1-1.51-1.31 6.04 6.04 0 0 0 0-7.82 1 1 0 0 1 .11-1.42Zm-2.5 1.84a1 1 0 0 1 1.4.12 4.8 4.8 0 0 1 0 5.98 1 1 0 0 1-1.52-1.3 2.8 2.8 0 0 0 0-3.38 1 1 0 0 1 .12-1.42Z"/></svg>`;
      case "battery-full":
        return `<svg viewBox="0 0 24 24" aria-hidden="true"><path fill="currentColor" d="M17 6h1a2 2 0 0 1 2 2v1h1v6h-1v1a2 2 0 0 1-2 2h-1v1H5V5h12v1Zm-1 2H7v8h9V8Z"/></svg>`;
      case "battery-good":
        return `<svg viewBox="0 0 24 24" aria-hidden="true"><path fill="currentColor" d="M17 6h1a2 2 0 0 1 2 2v1h1v6h-1v1a2 2 0 0 1-2 2h-1v1H5V5h12v1Zm-1 2H7v8h9V8Zm-1 1v6H8V9h7Z"/></svg>`;
      case "battery-low":
        return `<svg viewBox="0 0 24 24" aria-hidden="true"><path fill="currentColor" d="M17 6h1a2 2 0 0 1 2 2v1h1v6h-1v1a2 2 0 0 1-2 2h-1v1H5V5h12v1Zm-1 2H7v8h9V8Zm-4 1v6H8V9h4Z"/></svg>`;
      case "battery-caution":
        return `<svg viewBox="0 0 24 24" aria-hidden="true"><path fill="currentColor" d="M17 6h1a2 2 0 0 1 2 2v1h1v6h-1v1a2 2 0 0 1-2 2h-1v1H5V5h12v1Zm-1 2H7v8h9V8Zm-6.25 1h2.5v6h-2.5V9Z"/></svg>`;
      default:
        return `<svg viewBox="0 0 24 24" aria-hidden="true"><path fill="currentColor" d="M17 6h1a2 2 0 0 1 2 2v1h1v6h-1v1a2 2 0 0 1-2 2h-1v1H5V5h12v1Zm-1 2H7v8h9V8Z"/></svg>`;
    }
  }

  onMount(() => {
    window.meshHost?.send({
      kind: "register_frontend",
      component: {
        surface: "@mesh/navigation-bar",
        framework: "svelte",
        entry: "src/App.svelte",
        subscribes_to: subscribedBindables,
      },
    });

    for (const bindable of subscribedBindables) {
      window.meshHost?.subscribeBindable(bindable);
    }

    refreshBindableState();

    const unsubscribe = subscribedBindables.map((bindable) =>
      window.__meshBindableStore__?.subscribe(bindable, () => {
        refreshBindableState();
      }) ?? (() => {})
    );

    return () => {
      for (const dispose of unsubscribe) {
        dispose();
      }
    };
  });
</script>

<svelte:head>
  <title>MESH Navigation Bar</title>
</svelte:head>

<div class="nav-root">
  <nav class="nav-shell" aria-label="Navigation bar">
    <div class="meta">
      <span class="meta-label">{t("nav.current")}</span>
      <div class="meta-pill">
        <span class="meta-pill-text">{t("nav.dashboard")}</span>
      </div>

      <div class="action-slot">
        <button
          class="icon-button settings-button"
          on:click={onSettingsClick}
          title={t("nav.open_settings")}
          aria-label={t("nav.open_settings")}
        >
          <span class="glyph" aria-hidden="true">{@html iconSvg("settings")}</span>
        </button>
      </div>

      <div class="action-slot">
        <button
          class="icon-button volume-button"
          on:click={onVolumeClick}
          title={audioTooltip}
          aria-label={t("nav.open_audio")}
        >
          <span class="glyph" aria-hidden="true">{@html iconSvg(audioIconName)}</span>
        </button>
      </div>

      <div class="action-slot">
        <button
          class="battery-button"
          on:click={onBatteryClick}
          title={batteryTooltip}
          aria-label={batteryAriaLabel}
        >
          <span class="glyph battery-glyph" aria-hidden="true">{@html iconSvg(batteryIconName)}</span>
          <span class="battery-value">{batteryLabel}</span>
        </button>
      </div>
    </div>
  </nav>
</div>

<style>
  :global(html, body) {
    margin: 0;
    padding: 0;
    width: 100%;
    height: auto;
    overflow: hidden;
    background: transparent;
    font-family: "IBM Plex Sans", "Inter", sans-serif;
    user-select: none;
  }

  .nav-root {
    width: 100%;
    display: flex;
    flex-direction: column;
  }

  .nav-shell {
    box-sizing: border-box;
    width: 100%;
    min-height: 56px;
    display: flex;
    align-items: center;
    padding: 8px 14px;
    background:
      linear-gradient(180deg, rgba(13, 18, 24, 0.94), rgba(13, 18, 24, 0.9)),
      radial-gradient(circle at top left, rgba(73, 145, 165, 0.18), transparent 48%);
    border-bottom: 1px solid rgba(181, 214, 221, 0.15);
    color: rgba(244, 249, 250, 0.96);
    backdrop-filter: blur(22px);
  }

  .meta {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }

  .action-slot {
    position: relative;
    display: inline-flex;
    align-items: center;
  }

  .meta-label {
    width: 64px;
    color: rgba(196, 215, 219, 0.72);
    font-size: 0.74rem;
    letter-spacing: 0.03em;
    text-transform: uppercase;
  }

  .meta-pill {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 0.45rem 0.8rem;
    border-radius: 999px;
    background: rgba(111, 172, 186, 0.18);
    box-shadow: inset 0 0 0 1px rgba(133, 194, 207, 0.18);
  }

  .meta-pill-text {
    color: rgba(227, 246, 249, 0.94);
    font-size: 0.87rem;
    font-weight: 700;
  }

  .icon-button,
  .battery-button {
    border: 0;
    outline: none;
    cursor: pointer;
    transition:
      transform 140ms ease,
      background 140ms ease,
      color 140ms ease,
      border-radius 140ms ease;
  }

  .icon-button {
    width: 44px;
    height: 40px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border-radius: 12px;
    background: rgba(255, 255, 255, 0.075);
    color: rgba(239, 245, 246, 0.92);
  }

  .settings-button:hover {
    background: rgba(124, 199, 214, 0.2);
    color: white;
    border-radius: 999px;
    transform: translateY(-1px);
  }

  .volume-button:hover {
    background: rgba(255, 255, 255, 0.12);
    border-radius: 999px;
    transform: translateY(-1px);
  }

  .battery-button {
    height: 40px;
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
    padding: 0 0.8rem;
    border-radius: 999px;
    background: rgba(255, 255, 255, 0.075);
    color: rgba(239, 245, 246, 0.92);
  }

  .battery-button:hover {
    background: rgba(155, 206, 155, 0.18);
    color: white;
    transform: translateY(-1px);
  }

  .glyph {
    width: 1.3rem;
    height: 1.3rem;
    display: inline-flex;
    align-items: center;
    justify-content: center;
  }

  .glyph :global(svg) {
    width: 100%;
    height: 100%;
    display: block;
  }

  .battery-glyph {
    width: 1.15rem;
    height: 1.15rem;
  }

  .battery-value {
    font-size: 0.86rem;
    font-weight: 700;
    white-space: nowrap;
  }

  @media (max-width: 760px) {
    .meta-label,
    .meta-pill,
    .battery-value {
      display: none;
    }

    .battery-button {
      width: 44px;
      padding: 0;
      justify-content: center;
    }
  }
</style>
