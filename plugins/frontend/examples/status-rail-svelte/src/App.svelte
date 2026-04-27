<script lang="ts">
  import { onMount } from "svelte";

  let now = "waiting for core";

  onMount(() => {
    window.meshHost?.send({
      kind: "register_frontend",
      component: {
        surface: "@mesh/examples/status-rail-svelte",
        framework: "svelte",
        entry: "src/App.svelte",
        subscribes_to: ["time.now"],
      },
    });

    window.meshHost?.subscribeBindable("time.now");

    now = String(window.__meshBindableStore__?.get("time.now") ?? now);

    return (
      window.__meshBindableStore__?.subscribe("time.now", (value) => {
        now = String(value);
      }) ?? (() => {})
    );
  });

  function requestLauncher() {
    window.meshHost?.invokeCore("shell:open_launcher");
  }
</script>

<svelte:head>
  <title>MESH Status Rail</title>
</svelte:head>

<button class="rail" on:click={requestLauncher}>
  <span class="label">MESH</span>
  <span class="time">{now}</span>
</button>

<style>
  :global(body) {
    margin: 0;
    background: transparent;
    font-family: "IBM Plex Sans", sans-serif;
  }

  .rail {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    border: 0;
    border-radius: 999px;
    padding: 0.75rem 1rem;
    color: white;
    background:
      linear-gradient(135deg, rgba(19, 72, 92, 0.95), rgba(8, 29, 43, 0.92));
    box-shadow: 0 12px 32px rgba(0, 0, 0, 0.28);
  }

  .label {
    font-weight: 700;
    letter-spacing: 0.08em;
  }

  .time {
    font-variant-numeric: tabular-nums;
    opacity: 0.88;
  }
</style>
