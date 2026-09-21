import { FakeEngine, PositionModel, Scrubber } from "./scrubber.js";

const DURATION_MS = 3 * 60 * 1000 + 3 * 1000;
const FULLSCREEN_ORIGIN_ERROR_PX = 224;

const engine = new FakeEngine({ durationMs: DURATION_MS, latencyMs: 180 });

const variants = [
  { id: "variant-a", mode: "raw", commit: "down", keyboard: false, escCancel: false, dragThresholdPx: 0, buggy: true },
  { id: "variant-b", mode: "raw", commit: "release", keyboard: false, escCancel: false, dragThresholdPx: 0, buggy: false },
  { id: "variant-c", mode: "raw", commit: "move", keyboard: false, escCancel: false, dragThresholdPx: 0, buggy: false },
  { id: "variant-d", mode: "anchored", commit: "release", keyboard: true, escCancel: true, dragThresholdPx: 3, buggy: false },
];

const scrubbers = variants.map((variant) => {
  const root = document.getElementById(variant.id);
  const model = new PositionModel(engine, variant.mode);
  const scrubber = new Scrubber(root, {
    model,
    commit: variant.commit,
    dragThresholdPx: variant.dragThresholdPx,
    keyboard: variant.keyboard,
    escCancel: variant.escCancel,
  });
  return { ...variant, root, scrubber };
});

const PLAY_PATH = "M8 5v14l11-7z";
const PAUSE_PATH = "M7 5h3.5v14H7zm6.5 0H17v14h-3.5z";

const renderTransports = () => {
  for (const { root } of scrubbers) {
    const button = root.querySelector(".transport .play");
    button.querySelector("path").setAttribute("d", engine.playing ? PAUSE_PATH : PLAY_PATH);
    button.setAttribute("aria-label", engine.playing ? "Pause" : "Play");
  }
};

for (const { root } of scrubbers) {
  const transport = root.querySelector("[data-transport]");
  transport.querySelector(".play").addEventListener("click", () => {
    engine.setPlaying(!engine.playing);
    renderTransports();
  });
  transport.querySelector(".prev").addEventListener("click", () => {
    for (const { scrubber } of scrubbers) scrubber.model.seek(0);
  });
  transport.querySelector(".next").addEventListener("click", () => {
    for (const { scrubber } of scrubbers) scrubber.model.seek(DURATION_MS);
  });
}
renderTransports();

/*
 * Every variant drives the same engine, so a seek in one has to be mirrored into
 * the others' models or their bars drift apart.
 */
for (const { scrubber } of scrubbers) {
  const seek = scrubber.model.seek.bind(scrubber.model);
  scrubber.model.seek = (positionMs) => {
    for (const other of scrubbers) {
      if (other.scrubber.model === scrubber.model) continue;
      other.scrubber.model.adopt(positionMs);
    }
    seek(positionMs);
  };
}

document.getElementById("latency").addEventListener("change", (event) => {
  engine.latencyMs = event.target.checked ? 600 : 180;
});

document.getElementById("fullscreen").addEventListener("change", (event) => {
  for (const { scrubber, buggy } of scrubbers) {
    scrubber.originErrorPx = buggy && event.target.checked ? FULLSCREEN_ORIGIN_ERROR_PX : 0;
  }
});

Object.assign(window, { engine, scrubbers });
